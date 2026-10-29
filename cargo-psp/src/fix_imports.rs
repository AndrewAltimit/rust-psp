use anyhow::{Context, Result, ensure};
use goblin::elf::Elf;
use std::{collections::HashMap, path::Path};

/// A stub library entry like the one found in the `psp` crate, but with types
/// changed to be cross-platform.
///
/// In particular, pointers are swapped to `u32`.
#[derive(Debug, PartialEq)]
struct SceStubLibraryEntry {
    name: u32,
    version: [u8; 2],
    flags: u16,
    len: u8,
    v_stub_count: u8,
    stub_count: u16,
    nid_table: u32,
    stub_table: u32,
}

impl SceStubLibraryEntry {
    /// On-disk size: packed little-endian fields, no padding.
    const SIZE: usize = 20;

    fn from_le_bytes(b: &[u8]) -> Result<Self> {
        ensure!(
            b.len() == Self::SIZE,
            "stub entry is {} bytes, expected {}",
            b.len(),
            Self::SIZE
        );
        let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        Ok(Self {
            name: u32_at(0),
            version: [b[4], b[5]],
            flags: u16_at(6),
            len: b[8],
            v_stub_count: b[9],
            stub_count: u16_at(10),
            nid_table: u32_at(12),
            stub_table: u32_at(16),
        })
    }

    fn to_le_bytes(&self) -> [u8; Self::SIZE] {
        let mut out = [0u8; Self::SIZE];
        out[0..4].copy_from_slice(&self.name.to_le_bytes());
        out[4..6].copy_from_slice(&self.version);
        out[6..8].copy_from_slice(&self.flags.to_le_bytes());
        out[8] = self.len;
        out[9] = self.v_stub_count;
        out[10..12].copy_from_slice(&self.stub_count.to_le_bytes());
        out[12..16].copy_from_slice(&self.nid_table.to_le_bytes());
        out[16..20].copy_from_slice(&self.stub_table.to_le_bytes());
        out
    }
}

/// Fixes the stub count for all imported modules.
///
/// When linking, sometimes it is possible for imported functions to be
/// completely stripped. This can happen with LTO, for example. Because of this,
/// we need a way to write the stub count post-linking. This function does
/// exactly this.
pub fn fix<T: AsRef<Path>>(path: T) -> Result<()> {
    let path = path.as_ref();
    let mut bytes =
        std::fs::read(path).with_context(|| format!("failed to read ELF: {}", path.display()))?;
    let elf = Elf::parse(&bytes).context("failed to parse ELF")?;

    let shstrtab = {
        let sh = &elf.section_headers[elf.header.e_shstrndx as usize];
        let start = sh.sh_offset as usize;
        let end = (sh.sh_offset + sh.sh_size) as usize;

        &bytes[start..end]
    };

    // Map of name -> section header
    let sections = elf
        .section_headers
        .iter()
        .map(|sh| {
            let name = shstrtab[sh.sh_name..]
                .iter()
                .take_while(|b| **b != 0)
                .map(|b| *b as char)
                .collect::<String>();

            (name, sh)
        })
        .collect::<HashMap<_, _>>();

    let lib_stub = match sections.get(".lib.stub") {
        Some(s) => s,

        // The binary might not import any functions, in some rare cases.
        None => return Ok(()),
    };

    // TODO: Use module info instead of these sections, as they can technically
    // be stripped.

    // If we have .lib.stub, then .lib.stub.btm must exist.
    let lib_stub_btm = sections
        .get(".lib.stub.btm")
        .context("could not find .lib.stub.btm section")?;

    let rodata_sce_nid = sections
        .get(".rodata.sceNid")
        .context("could not find .rodata.sceNid section")?;

    let start = lib_stub.sh_offset as usize;
    let end = lib_stub_btm.sh_offset as usize;

    // Rough check for the length.
    ensure!(
        (end - start).is_multiple_of(SceStubLibraryEntry::SIZE),
        ".lib.stub section size ({}) is not a multiple of SceStubLibraryEntry size ({})",
        end - start,
        SceStubLibraryEntry::SIZE
    );

    // List of (stub index in .lib.stub, stub entry). Ordered by appearance in
    // .rodata.sceNid section.
    let stubs_nid_sorted = {
        let mut entries: Vec<(usize, SceStubLibraryEntry)> = bytes[start..end]
            .chunks(SceStubLibraryEntry::SIZE)
            .enumerate()
            .map(|(i, chunk)| Ok((i, SceStubLibraryEntry::from_le_bytes(chunk)?)))
            .collect::<Result<Vec<_>>>()?;

        // Sort by order of appearance in .rodata.sceNid section.
        entries.sort_unstable_by_key(|(_, s)| s.nid_table);

        entries
    };

    // Iterator of mutable byte slices (of stub lib entries) in raw ELF binary.
    let stub_entry_bufs = bytes[start..end].chunks_mut(SceStubLibraryEntry::SIZE);

    for (i, stub_entry_buf) in stub_entry_bufs.enumerate() {
        // A NID is a 32-bit value.
        const NID_SIZE: u32 = 4;

        let mut stub = SceStubLibraryEntry::from_le_bytes(stub_entry_buf)?;

        let sorted_pos = stubs_nid_sorted
            .iter()
            .position(|&(j, _)| i == j)
            .with_context(|| {
                format!("stub entry {i} not found in NID-sorted list (malformed PRX?)")
            })?;

        let nid_end = stubs_nid_sorted
            .get(1 + sorted_pos)
            .map(|(_, s)| s.nid_table)
            .unwrap_or(rodata_sce_nid.sh_addr as u32 + rodata_sce_nid.sh_size as u32);

        stub.stub_count = ((nid_end - stub.nid_table) / NID_SIZE) as u16;

        // Re-serialize the stub and save.
        stub_entry_buf.copy_from_slice(&stub.to_le_bytes());
    }

    std::fs::write(path, bytes)
        .with_context(|| format!("failed to write ELF: {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SceStubLibraryEntry;

    /// Layout must match what bincode 1.x (fixint, little-endian) produced.
    #[test]
    fn stub_entry_roundtrip_matches_legacy_layout() {
        let bytes: [u8; SceStubLibraryEntry::SIZE] = [
            0x78, 0x56, 0x34, 0x12, // name
            0x11, 0x00, // version
            0x09, 0x00, // flags
            0x05, // len
            0x00, // v_stub_count
            0x03, 0x00, // stub_count
            0xf0, 0xde, 0xbc, 0x9a, // nid_table
            0x44, 0x33, 0x22, 0x11, // stub_table
        ];
        let entry = SceStubLibraryEntry::from_le_bytes(&bytes).unwrap();
        assert_eq!(
            entry,
            SceStubLibraryEntry {
                name: 0x1234_5678,
                version: [0x11, 0x00],
                flags: 9,
                len: 5,
                v_stub_count: 0,
                stub_count: 3,
                nid_table: 0x9abc_def0,
                stub_table: 0x1122_3344,
            }
        );
        assert_eq!(entry.to_le_bytes(), bytes);
        assert!(SceStubLibraryEntry::from_le_bytes(&bytes[..19]).is_err());
    }
}

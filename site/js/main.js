/* rust-psp site: progressive enhancement only. The page is fully readable without JS. */
(function () {
  'use strict';

  var root = document.documentElement;
  var STORAGE_KEY = 'rust-psp-theme';

  /* ---------- Theme ---------- */
  var darkQuery = window.matchMedia ? window.matchMedia('(prefers-color-scheme: dark)') : null;

  function storedTheme() {
    try {
      var t = localStorage.getItem(STORAGE_KEY);
      return t === 'light' || t === 'dark' ? t : null;
    } catch (e) {
      return null;
    }
  }

  function resolvedTheme() {
    var explicit = root.getAttribute('data-theme');
    if (explicit === 'light' || explicit === 'dark') return explicit;
    return darkQuery && darkQuery.matches ? 'dark' : 'light';
  }

  var themeBtn = document.getElementById('theme-toggle');

  function syncThemeUi() {
    var current = resolvedTheme();
    root.setAttribute('data-theme-resolved', current);
    if (themeBtn) {
      var next = current === 'dark' ? 'light' : 'dark';
      themeBtn.setAttribute('aria-label', 'Switch to ' + next + ' theme');
      themeBtn.setAttribute('title', 'Switch to ' + next + ' theme');
    }
  }

  if (themeBtn) {
    themeBtn.addEventListener('click', function () {
      var next = resolvedTheme() === 'dark' ? 'light' : 'dark';
      root.setAttribute('data-theme', next);
      try { localStorage.setItem(STORAGE_KEY, next); } catch (e) { /* ignore */ }
      syncThemeUi();
    });
  }
  if (darkQuery) {
    var onSchemeChange = function () { if (!storedTheme()) syncThemeUi(); };
    if (darkQuery.addEventListener) darkQuery.addEventListener('change', onSchemeChange);
    else if (darkQuery.addListener) darkQuery.addListener(onSchemeChange);
  }
  syncThemeUi();

  /* ---------- Header: scrolled state + mobile menu ---------- */
  var header = document.querySelector('.site-header');
  function onScroll() {
    if (header) header.classList.toggle('scrolled', window.scrollY > 8);
  }
  window.addEventListener('scroll', onScroll, { passive: true });
  onScroll();

  var menuBtn = document.getElementById('menu-toggle');
  var nav = document.getElementById('primary-nav');
  function setMenu(open) {
    if (!menuBtn || !nav) return;
    menuBtn.setAttribute('aria-expanded', open ? 'true' : 'false');
    menuBtn.setAttribute('aria-label', open ? 'Close menu' : 'Open menu');
    nav.classList.toggle('open', open);
  }
  if (menuBtn && nav) {
    menuBtn.addEventListener('click', function () {
      setMenu(menuBtn.getAttribute('aria-expanded') !== 'true');
    });
    nav.addEventListener('click', function (e) {
      if (e.target.closest('a')) setMenu(false);
    });
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && menuBtn.getAttribute('aria-expanded') === 'true') {
        setMenu(false);
        menuBtn.focus();
      }
    });
  }

  /* ---------- Active section in nav ---------- */
  var navLinks = Array.prototype.slice.call(document.querySelectorAll('.primary-nav a[href^="#"]'));
  if ('IntersectionObserver' in window && navLinks.length) {
    var byId = {};
    navLinks.forEach(function (a) { byId[a.getAttribute('href').slice(1)] = a; });
    var observer = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (!entry.isIntersecting) return;
        navLinks.forEach(function (a) { a.removeAttribute('aria-current'); });
        var link = byId[entry.target.id];
        if (link) link.setAttribute('aria-current', 'true');
      });
    }, { rootMargin: '-45% 0px -50% 0px' });
    Object.keys(byId).forEach(function (id) {
      var el = document.getElementById(id);
      if (el) observer.observe(el);
    });
  }

  /* ---------- Syntax highlighting (tiny, purpose-built) ---------- */
  function escapeHtml(s) {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  var RULES = {
    rust: [
      ['tok-com', /\/\/[^\n]*/y],
      ['tok-str', /b?"(?:\\.|[^"\\])*"/y],
      ['tok-attr', /#!?\[[^\]\n]*\]/y],
      ['tok-mac', /[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*!/y],
      ['tok-kw', /\b(?:fn|let|mut|unsafe|for|in|if|else|match|return|use|mod|pub|struct|enum|impl|const|static|loop|while)\b/y],
      ['tok-num', /\b\d[\d_]*(?:\.\d+)?\b/y]
    ],
    toml: [
      ['tok-com', /#[^\n]*/y],
      ['tok-attr', /^\[[^\]\n]*\]/my],
      ['tok-str', /"(?:\\.|[^"\\])*"/y],
      ['tok-kw', /^[A-Za-z_][A-Za-z0-9_-]*(?=\s*=)/my]
    ],
    bash: [
      ['tok-com', /#[^\n]*/y],
      ['tok-str', /"(?:\\.|[^"\\])*"/y],
      ['tok-kw', /(?<![-\w\/.])(?:cargo|rustup|git|cd|cp|chmod|for|do|done)(?![-\w])/y]
    ]
  };

  function highlight(code, lang) {
    var rules = RULES[lang];
    if (!rules) return;
    var src = code.textContent;
    var out = '';
    var plain = '';
    var i = 0;
    outer: while (i < src.length) {
      for (var r = 0; r < rules.length; r++) {
        var re = rules[r][1];
        re.lastIndex = i;
        var m = re.exec(src);
        if (m && m.index === i && m[0].length) {
          // In bash, '#' only starts a comment at line start or after whitespace.
          if (lang === 'bash' && rules[r][0] === 'tok-com' && i > 0 && !/\s/.test(src[i - 1])) continue;
          out += escapeHtml(plain); plain = '';
          out += '<span class="' + rules[r][0] + '">' + escapeHtml(m[0]) + '</span>';
          i += m[0].length;
          continue outer;
        }
      }
      // Skip whole identifiers so keywords don't match inside words.
      var w = /[A-Za-z0-9_]+/y; w.lastIndex = i;
      var wm = w.exec(src);
      if (wm && wm.index === i) { plain += wm[0]; i += wm[0].length; continue; }
      plain += src[i]; i++;
    }
    out += escapeHtml(plain);
    code.innerHTML = out;
  }

  var supportsSticky = (function () { try { new RegExp('a', 'y'); return true; } catch (e) { return false; } })();

  /* ---------- Code blocks: highlight + copy button ---------- */
  var COPY_ICON = '<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3"/></svg>';
  var CHECK_ICON = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>';

  function copyText(text) {
    if (navigator.clipboard && window.isSecureContext) {
      return navigator.clipboard.writeText(text);
    }
    return new Promise(function (resolve, reject) {
      var ta = document.createElement('textarea');
      ta.value = text;
      ta.setAttribute('readonly', '');
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand('copy') ? resolve() : reject(new Error('copy failed')); }
      catch (e) { reject(e); }
      document.body.removeChild(ta);
    });
  }

  Array.prototype.forEach.call(document.querySelectorAll('.code'), function (block) {
    var code = block.querySelector('pre code');
    var head = block.querySelector('.code-head');
    var pre = block.querySelector('pre');
    if (!code) return;
    var raw = code.textContent;
    var lang = (code.className.match(/lang-(\w+)/) || [])[1];
    if (supportsSticky && lang) {
      try { highlight(code, lang); } catch (e) { code.textContent = raw; }
    }
    if (pre && pre.scrollWidth > pre.clientWidth) pre.setAttribute('tabindex', '0');

    if (!head) return;
    var btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'copy-btn';
    var label = head.textContent.trim();
    btn.setAttribute('aria-label', 'Copy ' + (label || 'code'));
    btn.innerHTML = COPY_ICON + '<span>Copy</span>';
    var timer;
    btn.addEventListener('click', function () {
      copyText(raw).then(function () {
        btn.classList.add('copied');
        btn.innerHTML = CHECK_ICON + '<span>Copied</span>';
      }, function () {
        btn.innerHTML = COPY_ICON + '<span>Press Ctrl+C</span>';
      }).then(function () {
        clearTimeout(timer);
        timer = setTimeout(function () {
          btn.classList.remove('copied');
          btn.innerHTML = COPY_ICON + '<span>Copy</span>';
        }, 1800);
      });
    });
    head.appendChild(btn);
  });

  /* ---------- Tabs (WAI-ARIA tabs pattern, automatic activation) ---------- */
  Array.prototype.forEach.call(document.querySelectorAll('[data-tabs]'), function (tabsEl) {
    var tabs = Array.prototype.slice.call(tabsEl.querySelectorAll('[role="tab"]'));
    function select(tab, focus) {
      tabs.forEach(function (t) {
        var on = t === tab;
        t.setAttribute('aria-selected', on ? 'true' : 'false');
        t.tabIndex = on ? 0 : -1;
        var panel = document.getElementById(t.getAttribute('aria-controls'));
        if (panel) panel.hidden = !on;
      });
      if (focus) tab.focus();
    }
    tabs.forEach(function (tab, idx) {
      tab.addEventListener('click', function () { select(tab, false); });
      tab.addEventListener('keydown', function (e) {
        var next = null;
        if (e.key === 'ArrowRight') next = tabs[(idx + 1) % tabs.length];
        else if (e.key === 'ArrowLeft') next = tabs[(idx - 1 + tabs.length) % tabs.length];
        else if (e.key === 'Home') next = tabs[0];
        else if (e.key === 'End') next = tabs[tabs.length - 1];
        if (next) { e.preventDefault(); select(next, true); }
      });
    });
  });

  /* ---------- Module filter ---------- */
  var search = document.getElementById('module-search');
  var moduleChips = Array.prototype.slice.call(document.querySelectorAll('[data-filter]'));
  var groups = Array.prototype.slice.call(document.querySelectorAll('.module-group'));
  var status = document.getElementById('module-status');
  var empty = document.getElementById('module-empty');
  var activeGroup = 'all';
  var totalModules = document.querySelectorAll('.module').length;

  function applyModuleFilter() {
    var q = search ? search.value.trim().toLowerCase() : '';
    var shown = 0;
    groups.forEach(function (g) {
      var groupOk = activeGroup === 'all' || g.getAttribute('data-group') === activeGroup;
      var visibleInGroup = 0;
      Array.prototype.forEach.call(g.querySelectorAll('.module'), function (m) {
        var ok = groupOk && (!q || m.textContent.toLowerCase().indexOf(q) !== -1);
        m.hidden = !ok;
        if (ok) visibleInGroup++;
      });
      g.hidden = visibleInGroup === 0;
      shown += visibleInGroup;
    });
    if (empty) empty.hidden = shown !== 0;
    if (status) {
      status.textContent = (q || activeGroup !== 'all')
        ? 'Showing ' + shown + ' of ' + totalModules + ' modules'
        : totalModules + ' modules';
    }
  }
  moduleChips.forEach(function (chip) {
    chip.addEventListener('click', function () {
      activeGroup = chip.getAttribute('data-filter');
      moduleChips.forEach(function (c) { c.setAttribute('aria-pressed', c === chip ? 'true' : 'false'); });
      applyModuleFilter();
    });
  });
  if (search) search.addEventListener('input', applyModuleFilter);
  if (groups.length) applyModuleFilter();

  /* ---------- Example filter ---------- */
  var exChips = Array.prototype.slice.call(document.querySelectorAll('[data-ex-filter]'));
  var examples = Array.prototype.slice.call(document.querySelectorAll('#example-list .example'));
  exChips.forEach(function (chip) {
    chip.addEventListener('click', function () {
      var f = chip.getAttribute('data-ex-filter');
      exChips.forEach(function (c) { c.setAttribute('aria-pressed', c === chip ? 'true' : 'false'); });
      examples.forEach(function (ex) {
        var cats = (ex.getAttribute('data-cat') || '').split(/\s+/);
        ex.hidden = f !== 'all' && cats.indexOf(f) === -1;
      });
    });
  });
})();

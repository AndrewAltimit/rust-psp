/* ============================================
   rust-psp GitHub Pages — Interactivity
   ============================================ */

(function () {
  'use strict';

  // --- Nav scroll effect ---
  const nav = document.querySelector('.nav');
  function updateNav() {
    nav.classList.toggle('scrolled', window.scrollY > 40);
  }
  window.addEventListener('scroll', updateNav, { passive: true });
  updateNav();

  // --- Smooth scrolling for anchor links ---
  document.querySelectorAll('a[href^="#"]').forEach(function (link) {
    link.addEventListener('click', function (e) {
      var target = document.querySelector(this.getAttribute('href'));
      if (target) {
        e.preventDefault();
        target.scrollIntoView({ behavior: 'smooth', block: 'start' });
        // Close mobile menu if open
        closeMobileMenu();
      }
    });
  });

  // --- Active nav link highlighting ---
  var sections = document.querySelectorAll('section[id]');
  var navLinks = document.querySelectorAll('.nav-links a, .nav-mobile a');

  var sectionObserver = new IntersectionObserver(
    function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          var id = entry.target.getAttribute('id');
          navLinks.forEach(function (link) {
            link.classList.toggle(
              'active',
              link.getAttribute('href') === '#' + id
            );
          });
        }
      });
    },
    { rootMargin: '-20% 0px -60% 0px' }
  );

  sections.forEach(function (section) {
    sectionObserver.observe(section);
  });

  // --- Mobile hamburger menu ---
  var hamburger = document.querySelector('.hamburger');
  var mobileMenu = document.querySelector('.nav-mobile');

  function closeMobileMenu() {
    if (hamburger && mobileMenu) {
      hamburger.classList.remove('open');
      mobileMenu.classList.remove('open');
      document.body.style.overflow = '';
    }
  }

  if (hamburger && mobileMenu) {
    hamburger.addEventListener('click', function () {
      var isOpen = hamburger.classList.toggle('open');
      mobileMenu.classList.toggle('open', isOpen);
      document.body.style.overflow = isOpen ? 'hidden' : '';
    });
  }

  // --- Scroll-triggered fade-in animations ---
  var fadeElements = document.querySelectorAll('.fade-in');
  var fadeObserver = new IntersectionObserver(
    function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add('visible');
          fadeObserver.unobserve(entry.target);
        }
      });
    },
    { threshold: 0.1, rootMargin: '0px 0px -40px 0px' }
  );

  fadeElements.forEach(function (el) {
    fadeObserver.observe(el);
  });

  // --- Module category expand/collapse ---
  document.querySelectorAll('.module-category-header').forEach(function (header) {
    header.addEventListener('click', function () {
      this.parentElement.classList.toggle('open');
    });
  });

  // --- Example gallery tag filtering ---
  var filterBtns = document.querySelectorAll('.filter-btn');
  var exampleCards = document.querySelectorAll('.example-card');

  filterBtns.forEach(function (btn) {
    btn.addEventListener('click', function () {
      var tag = this.getAttribute('data-tag');

      // Update active button
      filterBtns.forEach(function (b) { b.classList.remove('active'); });
      this.classList.add('active');

      // Filter cards
      exampleCards.forEach(function (card) {
        if (tag === 'all') {
          card.classList.remove('hidden');
        } else {
          var cardTags = card.getAttribute('data-tags');
          card.classList.toggle('hidden', !cardTags.includes(tag));
        }
      });
    });
  });
})();

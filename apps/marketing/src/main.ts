import "../styles.css";
import "../styles/features.css";

const nav = document.querySelector<HTMLElement>("[data-nav]");
const year = document.querySelector<HTMLElement>("[data-year]");
const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function updateNavigation(): void {
  if (nav !== null) {
    nav.classList.toggle("is-scrolled", window.scrollY > 8);
  }
}

function revealSections(): void {
  const sections = document.querySelectorAll<HTMLElement>("[data-reveal]");

  if (reduceMotion || !("IntersectionObserver" in window)) {
    for (const section of sections) {
      section.classList.add("is-visible");
    }
    return;
  }

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          entry.target.classList.add("is-visible");
          observer.unobserve(entry.target);
        }
      }
    },
    { rootMargin: "0px 0px -8% 0px" },
  );

  for (const section of sections) {
    observer.observe(section);
  }
}

if (year !== null) {
  year.textContent = String(new Date().getFullYear());
}

window.addEventListener("scroll", updateNavigation, { passive: true });
updateNavigation();
revealSections();

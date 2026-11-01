// Set the stored theme before first paint so the window never flashes the
// wrong ground. Kept as a file (not inline) so the CSP can forbid inline
// scripts. The React switch keeps this in step afterwards.
try {
  var saved = localStorage.getItem("akm-theme");
  if (saved === "light" || saved === "dark") {
    document.documentElement.dataset.theme = saved;
  } else if (window.matchMedia("(prefers-color-scheme: dark)").matches) {
    document.documentElement.dataset.theme = "dark";
  }
} catch (error) {
  // Storage can be refused; the system setting still applies via CSS.
}

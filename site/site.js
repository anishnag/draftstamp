// Install tabs (preselecting the visitor's platform) and copy buttons. No tracking.

(() => {
  const tabs = [...document.querySelectorAll(".tab")]
  const select = (id) => {
    for (const t of tabs) {
      const on = t.dataset.panel === id
      t.setAttribute("aria-selected", String(on))
      document.getElementById(t.dataset.panel).hidden = !on
    }
  }
  if (tabs.length) {
    tabs.forEach((t) => t.addEventListener("click", () => select(t.dataset.panel)))
    const ua = navigator.userAgent
    const guess = /Windows/.test(ua)
      ? "windows"
      : /Linux/.test(ua) && !/Android/.test(ua)
        ? /aarch64|arm64/i.test(ua) ? "linux-arm" : "linux-x64"
        : "mac-arm"
    select(guess)
  }

  for (const button of document.querySelectorAll(".copy")) {
    button.addEventListener("click", async () => {
      const text = button.parentElement.querySelector("code").innerText
      try {
        await navigator.clipboard.writeText(text)
        button.textContent = "Copied"
      } catch {
        button.textContent = "Select and copy"
      }
      setTimeout(() => (button.textContent = "Copy"), 1600)
    })
  }
})()

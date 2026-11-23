// Install tabs (preselecting the visitor's platform), copy buttons, and the docs table of contents.
// No tracking, no external requests.

(() => {
  const copyIcon =
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect width="14" height="14" x="8" y="8" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>'

  // Install tabs.
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
    select(
      /Windows/.test(ua)
        ? "windows"
        : /Linux/.test(ua) && !/Android/.test(ua)
          ? /aarch64|arm64/i.test(ua) ? "linux-arm" : "linux-x64"
          : "mac-arm",
    )
  }

  // A copy button on every code block. Comments (lines starting with #) are left out.
  for (const block of document.querySelectorAll(".codeblock")) {
    const button = document.createElement("button")
    button.className = "copy"
    button.type = "button"
    const label = () => (button.innerHTML = copyIcon + "<span>Copy</span>")
    label()
    button.addEventListener("click", async () => {
      const text = block
        .querySelector("pre")
        .innerText.split("\n")
        .filter((l) => !l.trim().startsWith("#"))
        .join("\n")
        .trim()
      try {
        await navigator.clipboard.writeText(text)
        button.innerHTML = copyIcon + "<span>Copied</span>"
      } catch {
        button.innerHTML = "<span>Select to copy</span>"
      }
      setTimeout(label, 1600)
    })
    block.prepend(button)
  }

  // Docs: highlight the section being read.
  const links = [...document.querySelectorAll(".toc a")]
  if (links.length && "IntersectionObserver" in window) {
    const byId = new Map(links.map((a) => [a.getAttribute("href").slice(1), a]))
    const visible = new Set()
    const update = () => {
      const first = [...byId.keys()].find((id) => visible.has(id))
      links.forEach((a) => a.classList.toggle("active", a === byId.get(first)))
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) e.isIntersecting ? visible.add(e.target.id) : visible.delete(e.target.id)
        update()
      },
      { rootMargin: "-80px 0px -60% 0px" },
    )
    byId.forEach((_, id) => {
      const section = document.getElementById(id)
      if (section) observer.observe(section)
    })
  }
})()

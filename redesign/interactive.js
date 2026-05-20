function setPressedFeedback(button) {
  button.classList.add("is-pressed");
  window.setTimeout(() => button.classList.remove("is-pressed"), 140);
}

function syncSlider(slider) {
  const value = Number(slider.value || 0);
  const min = Number(slider.min || 0);
  const max = Number(slider.max || 100);
  const percent = Math.round(((value - min) / (max - min)) * 100);
  const output = document.querySelector(`[data-slider-output="${slider.dataset.slider}"]`);
  if (output) {
    output.textContent = `${value}%`;
  }
  slider.setAttribute("aria-valuenow", String(value));
  slider.style.setProperty("--slider-percent", `${percent}%`);
}

document.addEventListener("click", (event) => {
  const button = event.target.closest("button");
  if (!button) return;

  setPressedFeedback(button);

  const disclosureButton = button.closest(".collapsible-section > .action-row__button");
  if (disclosureButton) {
    const section = disclosureButton.closest(".collapsible-section");
    const open = !section.classList.contains("is-open");
    section.classList.toggle("is-open", open);
    disclosureButton.setAttribute("aria-expanded", String(open));
  }

  const deviceDetailButton = button.closest(".device-detail-row__toggle");
  if (deviceDetailButton) {
    const row = deviceDetailButton.closest(".device-detail-row");
    const open = !row.classList.contains("is-open");
    row.classList.toggle("is-open", open);
    deviceDetailButton.setAttribute("aria-expanded", String(open));
  }

  if (button.matches('[role="switch"]')) {
    const checked = button.getAttribute("aria-checked") !== "true";
    button.setAttribute("aria-checked", String(checked));

    if (button.getAttribute("aria-label") === "Bluetooth enabled") {
      const subtitle = button.closest(".popover-shell")?.querySelector(".hero-row__subtitle");
      const onSubtitle = button.dataset.onSubtitle || "On";
      if (subtitle) subtitle.textContent = checked ? onSubtitle : "Off";
    }
  }

  if (button.matches('[role="checkbox"]')) {
    const checked = button.getAttribute("aria-checked") !== "true";
    button.setAttribute("aria-checked", String(checked));
    button.innerHTML = checked ? "&#10003;" : "";
  }

  if (button.dataset.selectGroup) {
    const group = button.dataset.selectGroup;
    document.querySelectorAll(`[data-select-group="${group}"]`).forEach((item) => {
      const selected = item === button;
      item.closest(".list-item, .action-row")?.classList.toggle("is-selected", selected);
      const right = item.querySelector(".list-item__right");
      if (right) {
        const idleRight = item.dataset.selectRight || "";
        const selectedRight = item.dataset.selectRightSelected || (idleRight ? `${idleRight} &#10003;` : "&#10003;");
        right.innerHTML = selected ? selectedRight : idleRight;
        right.setAttribute("aria-label", selected ? "Selected" : "");
      }
    });
    if (button.dataset.selectValue) {
      document.querySelectorAll(`[data-selection-label="${group}"]`).forEach((target) => {
        target.textContent = button.dataset.selectValue;
      });
    }
    if (button.dataset.selectMeta) {
      document.querySelectorAll(`[data-selection-meta="${group}"]`).forEach((target) => {
        target.textContent = button.dataset.selectMeta;
      });
    }
  }

  if (button.dataset.toggleMute) {
    const pressed = button.getAttribute("aria-pressed") !== "true";
    button.setAttribute("aria-pressed", String(pressed));
    button.classList.toggle("is-muted", pressed);
    button.textContent = pressed ? "Muted" : "Mute";
  }
});

document.addEventListener("input", (event) => {
  if (event.target.matches("input[data-slider]")) {
    syncSlider(event.target);
  }
});

document.querySelectorAll("input[data-slider]").forEach(syncSlider);

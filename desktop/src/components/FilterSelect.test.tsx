import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { FilterSelect } from "./FilterSelect";

afterEach(cleanup);

const options = [{ value: "alpha", label: "alpha" }, { value: "beta", label: "beta" }, { value: "bravo", label: "bravo" }];

function setup(value = "", disabled = false) {
  const onChange = vi.fn();
  render(<FilterSelect id="demo" label="Project" value={value} placeholder="Choose a project" options={options} onChange={onChange} disabled={disabled} />);
  return { onChange, field: screen.getByRole("combobox") as HTMLSelectElement };
}

test("a press opens AKM's list and keeps the system list closed", () => {
  const { field } = setup();
  const allowedSystemList = fireEvent.mouseDown(field, { detail: 1 });
  expect(allowedSystemList).toBe(false);
  expect(screen.getByRole("listbox").getAttribute("aria-label")).toBe("Project");
  expect(screen.getAllByRole("option", { hidden: false }).map((item) => item.textContent)).toContain("Choose a project");
  expect(field.getAttribute("aria-expanded")).toBe("true");
});

test("choosing an item reports it once and closes the list", () => {
  const { field, onChange } = setup();
  fireEvent.mouseDown(field, { detail: 1 });
  fireEvent.click(screen.getByRole("listbox").querySelectorAll("li")[2]);
  expect(onChange).toHaveBeenCalledTimes(1);
  expect(onChange).toHaveBeenCalledWith("beta");
  expect(screen.queryByRole("listbox")).toBeNull();
});

test("the current item is marked, and choosing it again reports nothing", () => {
  const { field, onChange } = setup("alpha");
  fireEvent.mouseDown(field, { detail: 1 });
  const alpha = screen.getByRole("listbox").querySelectorAll("li")[1];
  expect(alpha.getAttribute("aria-selected")).toBe("true");
  fireEvent.click(alpha);
  expect(onChange).not.toHaveBeenCalled();
});

test("an arrow key opens the list and does not change the value", () => {
  const { field, onChange } = setup("alpha");
  const allowedDefault = fireEvent.keyDown(field, { key: "ArrowDown" });
  expect(allowedDefault).toBe(false);
  expect(screen.getByRole("listbox")).toBeTruthy();
  expect(onChange).not.toHaveBeenCalled();
});

test("the keyboard moves through the list and Enter chooses", () => {
  const { field, onChange } = setup();
  fireEvent.keyDown(field, { key: "Enter" });
  fireEvent.keyDown(field, { key: "ArrowDown" });
  fireEvent.keyDown(field, { key: "ArrowDown" });
  const active = screen.getByRole("listbox").querySelector(".active");
  expect(active?.textContent).toBe("beta");
  expect(field.getAttribute("aria-activedescendant")).toBe(active?.id);
  fireEvent.keyDown(field, { key: "Enter" });
  expect(onChange).toHaveBeenCalledWith("beta");
});

test("typing moves to the item that starts with those letters", () => {
  const { field, onChange } = setup();
  fireEvent.keyDown(field, { key: " " });
  fireEvent.keyDown(field, { key: "b" });
  fireEvent.keyDown(field, { key: "r" });
  expect(screen.getByRole("listbox").querySelector(".active")?.textContent).toBe("bravo");
  expect(onChange).not.toHaveBeenCalled();
});

test("Escape closes only the list, and a dialog behind it does not hear the key", () => {
  const { field, onChange } = setup("alpha");
  const heard = vi.fn();
  document.addEventListener("keydown", heard);
  fireEvent.mouseDown(field, { detail: 1 });
  fireEvent.keyDown(field, { key: "Escape" });
  document.removeEventListener("keydown", heard);
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(heard).not.toHaveBeenCalled();
  expect(onChange).not.toHaveBeenCalled();
});

test("losing focus closes the list without a change", () => {
  const { field, onChange } = setup();
  fireEvent.mouseDown(field, { detail: 1 });
  fireEvent.blur(field);
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(onChange).not.toHaveBeenCalled();
});

test("a disabled field opens no list", () => {
  const { field } = setup("", true);
  fireEvent.mouseDown(field, { detail: 1 });
  expect(screen.queryByRole("listbox")).toBeNull();
});

test("a value set the system's way still reaches the app", () => {
  const { field, onChange } = setup();
  fireEvent.change(field, { target: { value: "beta" } });
  expect(onChange).toHaveBeenCalledWith("beta");
});

test("a press with no click count leaves the system's list to assistive tools", () => {
  const { field } = setup();
  const allowedSystemList = fireEvent.mouseDown(field, { detail: 0 });
  expect(allowedSystemList).toBe(true);
  expect(screen.queryByRole("listbox")).toBeNull();
});

function refreshing(initial: { value: string; label: string }[], value = "") {
  const onChange = vi.fn();
  const element = (next: { value: string; label: string }[], disabled = false) => (
    <FilterSelect id="demo" label="Project" value={value} placeholder="Choose a project" options={next} onChange={onChange} disabled={disabled} />
  );
  const view = render(element(initial));
  const field = screen.getByRole("combobox") as HTMLSelectElement;
  return { field, onChange, refresh: (next: { value: string; label: string }[], disabled = false) => view.rerender(element(next, disabled)) };
}
const named = (...names: string[]) => names.map((name) => ({ value: name, label: name }));
const activeLabel = () => screen.getByRole("listbox").querySelector(".active")?.textContent;

test("a refresh that adds an earlier option keeps the highlighted option, and Enter chooses it", () => {
  const { field, onChange, refresh } = refreshing(named("project-43", "project-44"));
  fireEvent.keyDown(field, { key: "Enter" });
  fireEvent.keyDown(field, { key: "End" });
  expect(activeLabel()).toBe("project-44");
  refresh(named("project-01", "project-43", "project-44"));
  expect(activeLabel()).toBe("project-44");
  fireEvent.keyDown(field, { key: "Enter" });
  expect(onChange).toHaveBeenCalledTimes(1);
  expect(onChange).toHaveBeenCalledWith("project-44");
});

test("a refresh that reorders the options keeps the highlighted option", () => {
  const { field, onChange, refresh } = refreshing(named("alpha", "beta", "gamma"));
  fireEvent.keyDown(field, { key: "Enter" });
  fireEvent.keyDown(field, { key: "ArrowDown" });
  expect(activeLabel()).toBe("alpha");
  refresh(named("gamma", "beta", "alpha"));
  expect(activeLabel()).toBe("alpha");
  fireEvent.keyDown(field, { key: "ArrowUp" });
  expect(activeLabel()).toBe("beta");
  fireEvent.keyDown(field, { key: "Enter" });
  expect(onChange).toHaveBeenCalledWith("beta");
});

test("a refresh that removes the highlighted option resets to the current value, and Enter changes nothing", () => {
  const { field, onChange, refresh } = refreshing(named("alpha", "beta", "gamma"), "alpha");
  fireEvent.keyDown(field, { key: "Enter" });
  fireEvent.keyDown(field, { key: "End" });
  expect(activeLabel()).toBe("gamma");
  refresh(named("alpha"));
  expect(activeLabel()).toBe("alpha");
  const active = screen.getByRole("listbox").querySelector(".active");
  expect(field.getAttribute("aria-activedescendant")).toBe(active?.id);
  expect(document.getElementById(field.getAttribute("aria-activedescendant") ?? "")).toBe(active);
  fireEvent.keyDown(field, { key: "Enter" });
  expect(onChange).not.toHaveBeenCalled();
});

test("a highlight that a refresh reset stays reset when the option returns", () => {
  const { field, refresh } = refreshing(named("alpha", "gamma"), "alpha");
  fireEvent.keyDown(field, { key: "Enter" });
  fireEvent.keyDown(field, { key: "End" });
  refresh(named("alpha"));
  refresh(named("alpha", "gamma"));
  expect(activeLabel()).toBe("alpha");
});

test("a field disabled while its list is open does not open the list again when it is enabled", () => {
  const { field, refresh } = refreshing(named("alpha"));
  fireEvent.keyDown(field, { key: "Enter" });
  expect(screen.queryByRole("listbox")).not.toBeNull();
  refresh(named("alpha"), true);
  expect(screen.queryByRole("listbox")).toBeNull();
  refresh(named("alpha"), false);
  expect(screen.queryByRole("listbox")).toBeNull();
});

test("only opening the list and a keyboard move scroll the list", () => {
  const scrolled = vi.fn();
  const original = Element.prototype.scrollIntoView;
  Element.prototype.scrollIntoView = scrolled;
  try {
    const { field } = setup("alpha");
    fireEvent.keyDown(field, { key: "Enter" });
    expect(scrolled).toHaveBeenCalledTimes(1);
    scrolled.mockClear();
    const list = screen.getByRole("listbox");
    fireEvent.scroll(list);
    fireEvent.mouseMove(list.querySelectorAll("li")[3]);
    expect(list.querySelectorAll("li")[3].className).toContain("active");
    expect(scrolled).not.toHaveBeenCalled();
    fireEvent.keyDown(field, { key: "ArrowUp" });
    expect(scrolled).toHaveBeenCalledTimes(1);
  } finally {
    Element.prototype.scrollIntoView = original;
  }
});

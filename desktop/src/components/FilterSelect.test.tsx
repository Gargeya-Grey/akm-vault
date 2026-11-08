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
  const allowedSystemList = fireEvent.mouseDown(field);
  expect(allowedSystemList).toBe(false);
  expect(screen.getByRole("listbox").getAttribute("aria-label")).toBe("Project");
  expect(screen.getAllByRole("option", { hidden: false }).map((item) => item.textContent)).toContain("Choose a project");
  expect(field.getAttribute("aria-expanded")).toBe("true");
});

test("choosing an item reports it once and closes the list", () => {
  const { field, onChange } = setup();
  fireEvent.mouseDown(field);
  fireEvent.click(screen.getByRole("listbox").querySelectorAll("li")[2]);
  expect(onChange).toHaveBeenCalledTimes(1);
  expect(onChange).toHaveBeenCalledWith("beta");
  expect(screen.queryByRole("listbox")).toBeNull();
});

test("the current item is marked, and choosing it again reports nothing", () => {
  const { field, onChange } = setup("alpha");
  fireEvent.mouseDown(field);
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
  fireEvent.mouseDown(field);
  fireEvent.keyDown(field, { key: "Escape" });
  document.removeEventListener("keydown", heard);
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(heard).not.toHaveBeenCalled();
  expect(onChange).not.toHaveBeenCalled();
});

test("losing focus closes the list without a change", () => {
  const { field, onChange } = setup();
  fireEvent.mouseDown(field);
  fireEvent.blur(field);
  expect(screen.queryByRole("listbox")).toBeNull();
  expect(onChange).not.toHaveBeenCalled();
});

test("a disabled field opens no list", () => {
  const { field } = setup("", true);
  fireEvent.mouseDown(field);
  expect(screen.queryByRole("listbox")).toBeNull();
});

test("a value set the system's way still reaches the app", () => {
  const { field, onChange } = setup();
  fireEvent.change(field, { target: { value: "beta" } });
  expect(onChange).toHaveBeenCalledWith("beta");
});

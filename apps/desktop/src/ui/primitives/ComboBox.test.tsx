import { useState } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { ComboBox } from "./ComboBox";

afterEach(cleanup);
const options = [
  { id: "all", label: "All", count: 3 },
  { id: "blocked", label: "Unavailable", isDisabled: true, hint: "Requires analysis" },
  { id: "html", label: "HTML", count: 2 },
];
function Fixture() {
  const [selected, setSelected] = useState("all");
  return <><button>Before</button><ComboBox label="Filter" options={options} selected={selected} onSelect={setSelected} /><button>After</button></>;
}

// spec: UI-126
it("keeps surrounding controls exposed, selects with the keyboard and skips disabled options", async () => {
  render(<Fixture />);
  const input = screen.getByRole("combobox", { name: "Filter" });
  fireEvent.focus(input);
  fireEvent.keyDown(input, { key: "ArrowDown" });
  await screen.findByRole("listbox", { name: "Filter" });
  expect(screen.getByRole("button", { name: "Before" }).closest('[aria-hidden="true"]')).toBeNull();
  expect(screen.getByRole("button", { name: "After" }).closest('[aria-hidden="true"]')).toBeNull();
  fireEvent.keyDown(input, { key: "ArrowDown" });
  const active = document.getElementById(input.getAttribute("aria-activedescendant")!);
  expect(active).toHaveTextContent("HTML");
  fireEvent.keyDown(input, { key: "Enter" });
  await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
  expect(input).toHaveValue("HTML (2)");
});

// spec: UI-126
it("filters by typing, restores the selection on Escape, and lets Tab leave the popup", async () => {
  render(<Fixture />);
  const input = screen.getByRole("combobox", { name: "Filter" });
  fireEvent.change(input, { target: { value: "ht" } });
  expect(await screen.findAllByRole("option")).toHaveLength(1);
  fireEvent.keyDown(input, { key: "Escape" });
  expect(input).toHaveValue("All (3)");
  fireEvent.keyDown(input, { key: "ArrowDown" });
  await screen.findByRole("listbox");
  expect(fireEvent.keyDown(input, { key: "Tab" })).toBe(true);
  await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
  expect(input).toHaveAttribute("aria-expanded", "false");
});

// spec: UI-126
it("lets the popup trigger close suggestions after focus moves from the input to that trigger", async () => {
  render(<Fixture />);
  const input = screen.getByRole("combobox", { name: "Filter" });
  const trigger = screen.getByRole("button", { name: "Filter" });
  fireEvent.keyDown(input, { key: "ArrowDown" });
  await screen.findByRole("listbox");
  fireEvent.blur(input, { relatedTarget: trigger });
  fireEvent.click(trigger);
  await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
});

// spec: UI-145
// spec: UI-143
it("builds no option list while closed (M2-N32)", async () => {
  let formatted = 0;
  const formatCount = (n: number) => { formatted++; return String(n); };
  const { rerender } = render(<ComboBox label="Filter" options={options} selected="blocked" onSelect={() => undefined} formatCount={formatCount} />);
  rerender(<ComboBox label="Filter" options={[...options]} selected="blocked" onSelect={() => undefined} formatCount={formatCount} />);
  expect(formatted).toBe(0);
  expect(document.querySelectorAll('[role="option"]')).toHaveLength(0);
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Filter" }), { key: "ArrowDown" });
  await screen.findByRole("listbox", { name: "Filter" });
  expect(screen.getAllByRole("option")).toHaveLength(3);
  expect(formatted).toBeGreaterThan(0);
});

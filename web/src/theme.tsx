import { useSyncExternalStore } from "react";
import { Select } from "./components/controls";
import { Field, FieldLabel } from "./components/ui/field";
export type Theme = "dark" | "light";
const readTheme = (): Theme =>
  document.documentElement.dataset.theme === "light" ? "light" : "dark";
const subscribe = (notify: () => void) => {
  window.addEventListener("nauctions-theme", notify);
  return () => window.removeEventListener("nauctions-theme", notify);
};
export function useTheme() {
  return useSyncExternalStore(subscribe, readTheme, () => "dark" as Theme);
}
export function applyTheme(value: string) {
  const theme: Theme = value === "light" ? "light" : "dark";
  document.documentElement.classList.toggle("dark", theme === "dark");
  document.documentElement.dataset.theme = theme;
  window.dispatchEvent(new Event("nauctions-theme"));
  try {
    localStorage.setItem("nauctions.theme", theme);
  } catch {
    /* Keep the session preference. */
  }
}
export function Appearance() {
  const theme = useTheme();
  return (
    <section className="settings-section">
      <h2>Appearance</h2>
      <Field>
        <FieldLabel htmlFor="appearance-theme">Theme</FieldLabel>
        <Select
          id="appearance-theme"
          value={theme}
          onChange={(e) => applyTheme(e.target.value)}
        >
          <option value="dark">Dark</option>
          <option value="light">Light</option>
        </Select>
      </Field>
    </section>
  );
}

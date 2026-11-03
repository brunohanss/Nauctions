// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { desktopInvoke, isDesktop } from "./desktop";
import { TestAlertButton } from "./TestAlertButton";

vi.mock("./desktop", () => ({ desktopInvoke: vi.fn(), isDesktop: vi.fn() }));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  vi.unstubAllGlobals();
});

it("requests the native test alert without a game-running check", async () => {
  vi.mocked(isDesktop).mockReturnValue(true);
  vi.mocked(desktopInvoke).mockResolvedValue(undefined);
  render(<TestAlertButton />);
  await userEvent.click(screen.getByRole("button", { name: "Test alert" }));
  expect(desktopInvoke).toHaveBeenCalledExactlyOnceWith("send_test_alert");
  expect(screen.getByRole("status").textContent).toContain(
    "Test alert requested",
  );
});

it("reports an older companion instead of claiming a notification was sent", async () => {
  vi.mocked(isDesktop).mockReturnValue(true);
  vi.mocked(desktopInvoke).mockRejectedValue(
    "Command send_test_alert not found",
  );
  render(<TestAlertButton />);
  await userEvent.click(screen.getByRole("button", { name: "Test alert" }));
  expect(screen.getByRole("alert").textContent).toContain(
    "Update the companion",
  );
  expect(screen.queryByRole("status")).toBeNull();
});

it("requests browser permission and sends a clearly marked test", async () => {
  const sent = vi.fn(),
    permission = vi.fn().mockResolvedValue("granted");
  vi.stubGlobal(
    "Notification",
    class {
      static permission = "default";
      static requestPermission = permission;
      constructor(title: string, options: NotificationOptions) {
        sent(title, options);
      }
    },
  );
  render(<TestAlertButton />);
  expect(permission).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Test alert" }));
  expect(permission).toHaveBeenCalledOnce();
  expect(sent).toHaveBeenCalledWith(
    "Nauctions test alert",
    expect.objectContaining({
      body: expect.stringContaining("test notification"),
    }),
  );
});

it("shows blocked permission and lets the user retry", async () => {
  const permission = vi.fn().mockResolvedValue("denied");
  vi.stubGlobal(
    "Notification",
    class {
      static permission = "default";
      static requestPermission = permission;
    },
  );
  render(<TestAlertButton />);
  await userEvent.click(screen.getByRole("button", { name: "Test alert" }));
  expect(screen.getByRole("alert").textContent).toContain(
    "Notifications are blocked",
  );
  expect(
    screen.getByRole("button", { name: "Test alert" }).hasAttribute("disabled"),
  ).toBe(false);
});

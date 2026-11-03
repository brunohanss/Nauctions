import { useState } from "react";
import { Button } from "./components/controls";
import { desktopInvoke, isDesktop } from "./desktop";

export function TestAlertButton() {
  const [busy, setBusy] = useState(false),
    [message, setMessage] = useState(""),
    [error, setError] = useState("");
  async function test() {
    if (busy) return;
    setBusy(true);
    setMessage("");
    setError("");
    try {
      if (isDesktop()) {
        try {
          await desktopInvoke("send_test_alert");
        } catch (failure) {
          const detail = String(failure);
          throw new Error(
            /not found|unknown command/i.test(detail)
              ? "Update the companion to use test alerts. This build does not support them."
              : detail,
          );
        }
      } else {
        if (!("Notification" in window))
          throw new Error(
            "This browser does not support notifications. Use the companion to test Windows alerts.",
          );
        const permission =
          Notification.permission === "default"
            ? await Notification.requestPermission()
            : Notification.permission;
        if (permission !== "granted")
          throw new Error(
            "Notifications are blocked. Allow notifications for this site in your browser settings, then try again.",
          );
        const notification = new Notification("Nauctions test alert", {
          body: "This is a test notification from your watchlist. No purchase action is needed.",
        });
        notification.onerror = () => {
          setMessage("");
          setError(
            "The browser could not show the test alert. Check your browser and Windows notification settings.",
          );
        };
      }
      setMessage(
        "Test alert requested. If it does not appear, check your notification settings.",
      );
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="watchlist-test-alert">
      <Button disabled={busy} onClick={() => void test()}>
        {busy ? "Sending test alert…" : "Test alert"}
      </Button>
      <p>Send one test notification, even when WoW is closed.</p>
      {message && <p role="status">{message}</p>}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}

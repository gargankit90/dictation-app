// U7: Settings → History, with the app's commands replaced by a fake backend.
// The texts here are made up.

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HistoryRow, Settings_Serialize } from "../bindings";
import { History, HISTORY_POLL_MS, limitQuestion, relativeTime } from "./History";

const MINUTE = 60_000;

function row(id: number, text: string, pasted = true, agoMinutes = id): HistoryRow {
  return { id: String(id), text, created_at: Date.now() - agoMinutes * MINUTE, pasted };
}

class Backend {
  limit = 25;
  entries: HistoryRow[] = [];
  calls: { command: string; args: Record<string, unknown> }[] = [];
  failing = new Map<string, string>();

  handle = (command: string, payload: unknown): unknown => {
    const args = (payload ?? {}) as Record<string, unknown>;
    this.calls.push({ command, args });
    const failure = this.failing.get(command);
    // Tauri rejects with the command's `Err` value, which is not an Error.
    // eslint-disable-next-line @typescript-eslint/only-throw-error
    if (failure !== undefined) throw failure;
    switch (command) {
      case "list_history":
        return this.entries;
      case "get_settings":
        return settingsWith(this.limit);
      case "set_history_limit":
        this.limit = args.limit as number;
        this.entries = this.entries.slice(0, this.limit);
        return null;
      case "clear_history":
        this.entries = [];
        return null;
      case "delete_history_entry":
        this.entries = this.entries.filter((entry) => entry.id !== args.id);
        return null;
      case "copy_history_entry":
        return null;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  };

  called(command: string) {
    return this.calls.filter((call) => call.command === command);
  }
}

function settingsWith(limit: number, hotkey = "RightAlt"): Settings_Serialize {
  return {
    version: 2,
    provider: "groq",
    model: "whisper-large-v3-turbo",
    language: null,
    hotkey,
    launch_at_login: false,
    transcription: "cloud",
    local_model: "m",
    share_usage_events: true,
    history_limit: limit,
  };
}

let backend: Backend;

/** Renders the section the way Settings does: `onStored` feeds `settings` back. */
function Harness({ hotkey }: { hotkey?: string }) {
  const [settings, setSettings] = useState(settingsWith(backend.limit, hotkey));
  return <History settings={settings} onStored={setSettings} />;
}

async function show(hotkey?: string) {
  render(<Harness hotkey={hotkey} />);
  await waitFor(() => {
    expect(backend.called("list_history").length).toBeGreaterThan(0);
  });
}

beforeEach(() => {
  backend = new Backend();
  mockIPC((command, payload) => backend.handle(command, payload));
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.useRealTimers();
});

describe("relative times", () => {
  const now = new Date(2026, 10, 4, 15, 0).getTime();

  it("criterion 5: says how long ago, then the day and time", () => {
    expect(relativeTime(now - 10_000, now)).toBe("Just now");
    expect(relativeTime(now - 2 * MINUTE, now)).toBe("2 min ago");
    expect(relativeTime(now - 59 * MINUTE, now)).toBe("59 min ago");
    expect(relativeTime(new Date(2026, 10, 4, 9, 7).getTime(), now)).toBe("Today 09:07");
    expect(relativeTime(new Date(2026, 10, 3, 14, 5).getTime(), now)).toBe("Yesterday 14:05");
    expect(relativeTime(new Date(2026, 10, 1, 14, 5).getTime(), now)).toMatch(/1.*14:05$/);
    expect(relativeTime(new Date(2025, 10, 1, 14, 5).getTime(), now)).toMatch(/2025.*14:05$/);
  });

  it("a clock set back shows no negative time", () => {
    expect(relativeTime(now + 5 * MINUTE, now)).toBe("Just now");
  });
});

describe("what lowering the limit asks", () => {
  it("criterion 3: says how many will go, and asks nothing if none will", () => {
    expect(limitQuestion(10, 25)).toBe(
      "Keep the last 10 and delete the 15 oldest dictations? This can't be undone.",
    );
    expect(limitQuestion(10, 11)).toBe(
      "Keep the last 10 and delete the oldest dictation? This can't be undone.",
    );
    expect(limitQuestion(0, 25)).toBe(
      "Turn off history and delete all 25 dictations? This can't be undone.",
    );
    expect(limitQuestion(0, 1)).toBe(
      "Turn off history and delete 1 dictation? This can't be undone.",
    );
    expect(limitQuestion(10, 10)).toBeNull();
    expect(limitQuestion(100, 25)).toBeNull();
    expect(limitQuestion(0, 0)).toBeNull();
  });
});

describe("Settings → History", () => {
  it("criterion 5: the empty state names the user's own key, and there is nothing to clear", async () => {
    await show();
    expect(await screen.findByText("No dictations yet. Hold Right ⌥ and speak.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Clear history…" })).toBeNull();
    expect(screen.queryByRole("list")).toBeNull();
    cleanup();
    await show("LeftCmd");
    expect(await screen.findByText("No dictations yet. Hold Left ⌘ and speak.")).toBeTruthy();
  });

  it("offers Off, 10, 25, 50, 100 with the caption and the current choice", async () => {
    await show();
    const select = screen.getByLabelText<HTMLSelectElement>("Keep recent dictations");
    expect(select.value).toBe("25");
    expect([...select.options].map((option) => option.textContent)).toEqual([
      "Off",
      "10",
      "25",
      "50",
      "100",
    ]);
    expect(screen.getByText("Text only, kept on this Mac, never sent.")).toBeTruthy();
    expect(select.getAttribute("aria-describedby")).toBe("history-limit-hint");
  });

  it("criterion 5: lists newest first with the time, the text and the chip", async () => {
    backend.entries = [row(2, "the second one", false), row(5, "the first one")];
    await show();
    const list = await screen.findByRole("list", { name: "Recent dictations" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    const [first, second] = rows as [HTMLElement, HTMLElement];
    expect(rows[0]?.textContent).toContain("2 min ago");
    expect(rows[0]?.textContent).toContain("the second one");
    expect(within(first).getByText("Not pasted")).toBeTruthy();
    expect(rows[1]?.textContent).toContain("5 min ago");
    expect(within(second).queryByText("Not pasted")).toBeNull();
    // The preview is cut by the stylesheet, two lines; the class carries it.
    expect(within(first).getByText("the second one").className).toBe("history-text");
  });

  it("a new dictation appears while the section shows", async () => {
    await show();
    await screen.findByText(/No dictations yet/);
    backend.entries = [row(1, "just dictated")];
    expect(
      await screen.findByText("just dictated", {}, { timeout: HISTORY_POLL_MS + 1_000 }),
    ).toBeTruthy();
  });

  it("criterion 5: Copy asks the app to copy that entry, and says so", async () => {
    backend.entries = [row(1, "one"), row(2, "two")];
    await show();
    fireEvent.click(await screen.findByRole("button", { name: "Copy dictation from 2 min ago" }));
    await waitFor(() => {
      expect(backend.called("copy_history_entry")[0]?.args).toEqual({ id: "2" });
    });
    expect((await screen.findAllByText("Copied")).length).toBeGreaterThan(0);
  });

  it("a failed Copy says why", async () => {
    backend.entries = [row(1, "one")];
    backend.failing.set("copy_history_entry", "The clipboard isn't available");
    await show();
    fireEvent.click(await screen.findByRole("button", { name: /^Copy dictation/ }));
    expect((await screen.findByRole("alert")).textContent).toContain(
      "The clipboard isn't available",
    );
  });

  it("criterion 5: Delete removes one entry and the keyboard moves to the next", async () => {
    backend.entries = [row(1, "one"), row(2, "two"), row(3, "three")];
    await show();
    fireEvent.click(await screen.findByRole("button", { name: "Delete dictation from 2 min ago" }));
    await waitFor(() => {
      expect(screen.queryByText("two")).toBeNull();
    });
    expect(backend.called("delete_history_entry")[0]?.args).toEqual({ id: "2" });
    expect(screen.getByText("one")).toBeTruthy();
    await waitFor(() => {
      expect(document.activeElement?.getAttribute("aria-label")).toBe(
        "Delete dictation from 3 min ago",
      );
    });
  });

  it("deleting the last entry shows the empty state and keeps the keyboard in the section", async () => {
    backend.entries = [row(1, "one")];
    await show();
    fireEvent.click(await screen.findByRole("button", { name: /^Delete dictation/ }));
    await screen.findByText(/No dictations yet/);
    await waitFor(() => {
      expect(document.activeElement?.id).toBe("history-limit");
    });
  });

  it("criterion 5: every action is a real, named button, focusable in order", async () => {
    backend.entries = [row(1, "one")];
    await show();
    const copy = await screen.findByRole("button", { name: "Copy dictation from 1 min ago" });
    const remove = screen.getByRole("button", { name: "Delete dictation from 1 min ago" });
    for (const button of [copy, remove]) {
      expect(button.tagName).toBe("BUTTON");
      expect(button.getAttribute("tabindex")).toBeNull();
      button.focus();
      expect(document.activeElement).toBe(button);
    }
    // Names never hold the dictated text.
    expect(copy.getAttribute("aria-label")).not.toContain("one");
  });

  it("criterion 3: Clear history asks with the count, and Cancel keeps everything", async () => {
    backend.entries = Array.from({ length: 25 }, (_, n) => row(n + 1, `text ${String(n)}`));
    await show();
    const clear = await screen.findByRole("button", { name: "Clear history…" });
    fireEvent.click(clear);
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain("Delete all 25 dictations? This can't be undone.");
    expect(document.activeElement?.textContent).toBe("Cancel");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(backend.called("clear_history")).toHaveLength(0);
    expect(document.activeElement).toBe(clear);
  });

  it("criterion 3: confirming Clear history deletes all", async () => {
    backend.entries = [row(1, "one"), row(2, "two")];
    await show();
    fireEvent.click(await screen.findByRole("button", { name: "Clear history…" }));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await screen.findByText(/No dictations yet/);
    expect(backend.called("clear_history")).toHaveLength(1);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("the question works by keyboard: Esc cancels and Tab stays inside", async () => {
    backend.entries = [row(1, "one")];
    await show();
    fireEvent.click(await screen.findByRole("button", { name: "Clear history…" }));
    const dialog = await screen.findByRole("alertdialog");
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    const confirm = within(dialog).getByRole("button", { name: "Delete" });
    expect(document.activeElement).toBe(cancel);
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(document.activeElement).toBe(confirm);
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(document.activeElement).toBe(cancel);
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(backend.called("clear_history")).toHaveLength(0);
  });

  it("criterion 3: lowering the limit asks how many will go, then trims", async () => {
    backend.entries = Array.from({ length: 25 }, (_, n) => row(n + 1, `text ${String(n)}`));
    await show();
    const select = await screen.findByLabelText<HTMLSelectElement>("Keep recent dictations");
    fireEvent.change(select, { target: { value: "10" } });
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain("Keep the last 10 and delete the 15 oldest dictations?");
    // Nothing yet; the control still shows what is stored.
    expect(backend.called("set_history_limit")).toHaveLength(0);
    expect(select.value).toBe("25");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => {
      expect(backend.called("set_history_limit")[0]?.args).toEqual({ limit: 10 });
    });
    await waitFor(() => {
      expect(select.value).toBe("10");
    });
    await waitFor(() => {
      expect(screen.getAllByRole("listitem")).toHaveLength(10);
    });
  });

  it("criterion 3: cancelling keeps the limit and the entries", async () => {
    backend.entries = Array.from({ length: 12 }, (_, n) => row(n + 1, `text ${String(n)}`));
    await show();
    const select = await screen.findByLabelText<HTMLSelectElement>("Keep recent dictations");
    fireEvent.change(select, { target: { value: "10" } });
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(backend.called("set_history_limit")).toHaveLength(0);
    expect(select.value).toBe("25");
    expect(document.activeElement).toBe(select);
  });

  it("the count is read when asked, so a dictation that just arrived counts", async () => {
    backend.entries = Array.from({ length: 10 }, (_, n) => row(n + 1, `text ${String(n)}`));
    await show();
    await screen.findAllByRole("listitem");
    backend.entries = [row(0, "newer", true, 0), ...backend.entries];
    fireEvent.change(screen.getByLabelText("Keep recent dictations"), { target: { value: "10" } });
    expect((await screen.findByRole("alertdialog")).textContent).toContain("the oldest dictation");
  });

  it("raising the limit applies at once, with no question and no Save", async () => {
    backend.entries = [row(1, "one")];
    await show();
    const select = await screen.findByLabelText<HTMLSelectElement>("Keep recent dictations");
    fireEvent.change(select, { target: { value: "50" } });
    await waitFor(() => {
      expect(backend.called("set_history_limit")[0]?.args).toEqual({ limit: 50 });
    });
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
    expect(await screen.findByText("Saved")).toBeTruthy();
  });

  it("criterion 3: Off asks, deletes all and says history is off", async () => {
    backend.entries = [row(1, "one"), row(2, "two")];
    await show();
    fireEvent.change(await screen.findByLabelText("Keep recent dictations"), {
      target: { value: "0" },
    });
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain("Turn off history and delete all 2 dictations?");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("History is off. New dictations aren't saved.")).toBeTruthy();
    expect(backend.called("set_history_limit")[0]?.args).toEqual({ limit: 0 });
  });

  it("a limit the app can't save says why and the control shows what is stored", async () => {
    backend.failing.set("set_history_limit", "settings file: disk full");
    await show();
    const select = await screen.findByLabelText<HTMLSelectElement>("Keep recent dictations");
    fireEvent.change(select, { target: { value: "50" } });
    expect((await screen.findByRole("alert")).textContent).toContain("disk full");
    expect(select.value).toBe("25");
  });

  it("a list the app can't read says so", async () => {
    backend.failing.set("list_history", "boom");
    await show();
    expect((await screen.findByRole("alert")).textContent).toContain("boom");
  });

  it("times keep up while the section shows", async () => {
    backend.entries = [row(1, "one", true, 1)];
    await show();
    await screen.findByText("1 min ago");
    vi.useFakeTimers({ toFake: ["setInterval", "Date"] });
    cleanup();
    render(<Harness />);
    await act(async () => {
      await Promise.resolve();
    });
    vi.setSystemTime(Date.now() + 2 * MINUTE);
    act(() => {
      vi.advanceTimersByTime(HISTORY_POLL_MS);
    });
    vi.useRealTimers();
    expect(await screen.findByText("3 min ago")).toBeTruthy();
  });
});

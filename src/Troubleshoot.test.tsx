// E1 criteria 2, 3 and 6: Troubleshoot's checks and Report a problem, with
// the app's commands replaced by a fake backend through `mockIPC`.

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Check } from "./bindings";
import { Troubleshoot } from "./Troubleshoot";

const REPORT = [
  "Vocet diagnostics report",
  "app_version: 0.1.0",
  "microphone: denied",
  "recent_errors:",
  "  - 2026-11-03T10:11:12Z mic_denied",
  "",
].join("\n");

function check(partial: Partial<Check> & Pick<Check, "id" | "label">): Check {
  return {
    ok: true,
    detail: "Allowed.",
    code: null,
    details: partial.code ?? null,
    fix: null,
    fix_label: null,
    local_model: null,
    ...partial,
  };
}

const HEALTHY: Check[] = [
  check({ id: "microphone", label: "Microphone" }),
  check({ id: "accessibility", label: "Accessibility" }),
  check({ id: "hotkey", label: "Dictation key", detail: "Registered." }),
  check({ id: "engine", label: "Speech engine", detail: "Ready." }),
  check({ id: "last_error", label: "Last error", detail: "None since the app started." }),
];

const MIC_DENIED: Check[] = [
  check({
    id: "microphone",
    label: "Microphone",
    ok: false,
    detail:
      "Vocet can't use the microphone. Allow it in System Settings → Privacy & Security → Microphone.",
    code: "mic_denied",
    fix: "open_microphone_settings",
    fix_label: "Open Microphone Settings",
  }),
  ...HEALTHY.slice(1),
];

let calls: string[] = [];
let args: Record<string, unknown>[] = [];
let checks: Check[] = HEALTHY;
let connection = { ok: true, code: null as string | null, message: "The provider answered." };

function backend(command: string, payload: unknown): unknown {
  calls.push(command);
  args.push((payload ?? {}) as Record<string, unknown>);
  switch (command) {
    case "run_checks":
      return checks;
    case "test_connection":
      return connection;
    case "build_report":
      return REPORT;
    case "save_report":
      return "vocet-report.txt";
    case "request_permission":
      return {
        view: "settings",
        step: "microphone",
        microphone: "denied",
        accessibility: "granted",
      };
    default:
      return null;
  }
}

function view(props: Partial<Parameters<typeof Troubleshoot>[0]> = {}) {
  return render(
    <Troubleshoot
      onBack={() => undefined}
      onOpenSettings={() => undefined}
      onReport={() => undefined}
      {...props}
    />,
  );
}

beforeEach(() => {
  calls = [];
  args = [];
  checks = HEALTHY;
  connection = { ok: true, code: null, message: "The provider answered." };
  mockIPC(backend);
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

describe("criterion_e1_2 Troubleshoot checks", () => {
  it("shows a tick for each passing check and runs nothing else on its own", async () => {
    view();
    const list = await screen.findByRole("list", { name: "Checks" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows).toHaveLength(5);
    expect(within(list).getAllByRole("img", { name: "OK" })).toHaveLength(5);
    expect(within(list).queryByRole("button")).toBeNull();
    // Opening it contacts nobody and builds no report.
    expect(calls).toEqual(["run_checks"]);
  });

  it("shows a cross, the code and a working fix button for a denied microphone", async () => {
    checks = MIC_DENIED;
    view();
    const list = await screen.findByRole("list", { name: "Checks" });
    expect(within(list).getAllByRole("img", { name: "Problem" })).toHaveLength(1);
    expect(within(list).getByText(/Vocet can't use the microphone/).textContent).not.toContain(
      "mic_denied",
    );
    expect(within(list).getByText("mic_denied").closest("details")).toBeTruthy();
    fireEvent.click(within(list).getByRole("button", { name: "Open Microphone Settings" }));
    await waitFor(() => {
      expect(calls).toContain("request_permission");
    });
    expect(args[calls.indexOf("request_permission")]).toEqual({ permission: "microphone" });
    // And it checks again afterwards.
    await waitFor(() => {
      expect(calls.filter((call) => call === "run_checks")).toHaveLength(2);
    });
  });

  it("opens Accessibility settings and the settings form from their fix buttons", async () => {
    const onOpenSettings = vi.fn();
    checks = [
      check({
        id: "accessibility",
        label: "Accessibility",
        ok: false,
        detail: "Accessibility is off.",
        code: "accessibility_missing",
        fix: "open_accessibility_settings",
        fix_label: "Open Accessibility Settings",
      }),
      check({
        id: "engine",
        label: "Speech engine",
        ok: false,
        detail: "No API key is saved.",
        code: "key_missing",
        fix: "open_settings",
        fix_label: "Open Settings",
      }),
      check({
        id: "last_error",
        label: "Last error",
        ok: false,
        detail: "No internet connection. Check your internet connection.",
        code: "network_unreachable",
        details: "network_unreachable at 2026-11-03T10:11:12Z",
        fix: "try_again",
        fix_label: "Try again",
      }),
    ];
    view({ onOpenSettings });
    fireEvent.click(await screen.findByRole("button", { name: "Open Accessibility Settings" }));
    await waitFor(() => {
      expect(args[calls.indexOf("request_permission")]).toEqual({ permission: "accessibility" });
    });
    fireEvent.click(screen.getByRole("button", { name: "Open Settings" }));
    expect(onOpenSettings).toHaveBeenCalledOnce();
    // Details hold the last error's code and time, and "try again" has no button.
    expect(screen.getByText(/network_unreachable at 2026-11-03T10:11:12Z/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  });

  it("tests the connection only when asked, and shows the code of a failure", async () => {
    connection = {
      ok: false,
      code: "key_rejected",
      message: "The provider rejected your API key.",
    };
    view();
    await screen.findByRole("list", { name: "Checks" });
    expect(calls).not.toContain("test_connection");
    fireEvent.click(screen.getByRole("button", { name: "Test connection" }));
    const status = await screen.findByText(/The provider rejected your API key/);
    expect(status.textContent).not.toContain("key_rejected");
    expect(screen.getByText("key_rejected").closest("details")?.open).toBe(false);
    expect(calls.filter((call) => call === "test_connection")).toHaveLength(1);
  });

  it("U1 criterion 8: hides Test connection for the local model and shows its readiness", async () => {
    checks = HEALTHY.map((row) =>
      row.id === "engine"
        ? { ...row, local_model: "Ready: the model is downloaded and loaded." }
        : row,
    );
    view();
    await screen.findByRole("list", { name: "Checks" });
    expect(screen.queryByRole("button", { name: "Test connection" })).toBeNull();
    expect(screen.getByTestId("local-readiness").textContent).toContain(
      "Ready: the model is downloaded and loaded.",
    );
  });

  it("U1 round 2: a denied microphone offers Relaunch Vocet, because a grant needs one", async () => {
    checks = MIC_DENIED;
    view();
    await screen.findByRole("list", { name: "Checks" });
    fireEvent.click(screen.getByRole("button", { name: "Relaunch Vocet" }));
    await waitFor(() => {
      expect(calls).toContain("relaunch_app");
    });
  });

  it("U1 round 4: a failed relaunch shows the error with its next step", async () => {
    checks = MIC_DENIED;
    const failing = (command: string, payload: unknown): unknown => {
      if (command === "relaunch_app") {
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw "Couldn't prepare the relaunch. Quit Vocet from the menu bar and open it again.";
      }
      return backend(command, payload);
    };
    mockIPC(failing);
    view();
    await screen.findByRole("list", { name: "Checks" });
    fireEvent.click(screen.getByRole("button", { name: "Relaunch Vocet" }));
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBe(
      "Couldn't prepare the relaunch. Quit Vocet from the menu bar and open it again.",
    );
  });

  it("U1 round 4: a relaunch that can't be reached says what to do instead", async () => {
    checks = MIC_DENIED;
    mockIPC((command, payload) => {
      if (command === "relaunch_app") throw new Error("the bridge is gone");
      return backend(command, payload);
    });
    view();
    await screen.findByRole("list", { name: "Checks" });
    fireEvent.click(screen.getByRole("button", { name: "Relaunch Vocet" }));
    expect((await screen.findByRole("alert")).textContent).toContain(
      "Quit it from the menu bar and open it again",
    );
  });

  it("U1 criterion 8: still offers Test connection for a cloud provider", async () => {
    view();
    await screen.findByRole("list", { name: "Checks" });
    expect(screen.getByRole("button", { name: "Test connection" })).toBeTruthy();
    expect(screen.queryByTestId("local-readiness")).toBeNull();
  });

  it("says when the checks can't run", async () => {
    mockIPC((command) => {
      if (command === "run_checks") throw new Error("the check task crashed");
      return null;
    });
    view();
    expect((await screen.findByRole("alert")).textContent).toContain("Couldn't run the checks");
  });
});

describe("u12_criterion_2 Troubleshoot is checks and fixes only", () => {
  it("has no report and no report actions; only a failed check links to Report", async () => {
    const onReport = vi.fn();
    view({ onReport });
    await screen.findByRole("list", { name: "Checks" });
    for (const gone of [
      "Open a GitHub issue",
      "Save report…",
      "Copy",
      "Preview report",
      "Show log file",
    ]) {
      expect(screen.queryByRole("button", { name: gone }), gone).toBeNull();
    }
    expect(screen.queryByLabelText("Report")).toBeNull();
    expect(screen.queryByRole("button", { name: "Report a problem…" })).toBeNull();
    expect(calls).not.toContain("build_report");
    cleanup();
    checks = MIC_DENIED;
    view({ onReport });
    fireEvent.click(await screen.findByRole("button", { name: "Report a problem…" }));
    expect(onReport).toHaveBeenCalledOnce();
    expect(calls).not.toContain("build_report");
  });

  it("keeps the code behind a closed Details", async () => {
    checks = MIC_DENIED;
    view();
    const row = (await screen.findByText("Microphone")).closest("li");
    if (row === null) throw new Error("no row");
    const details = row.querySelector("details");
    expect(details?.open).toBe(false);
    expect(details?.textContent).toContain("mic_denied");
    expect(row.querySelector(".check-body p")?.textContent).not.toContain("mic_denied");
  });

  it("goes back to Settings", async () => {
    const onBack = vi.fn();
    view({ onBack });
    fireEvent.click(await screen.findByRole("button", { name: "Back to Settings" }));
    expect(onBack).toHaveBeenCalledOnce();
  });
});

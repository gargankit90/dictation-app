import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

afterEach(cleanup);

describe("App", () => {
  it("shows the app name as the main heading", () => {
    render(<App />);

    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("dictation-app");
  });
});

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ToolMessageAdapter from "./tool-adapter";

const call = {
  id: "call-1",
  name: "memory_read",
  arguments: { path: "notes/today.md" },
};

describe("ToolMessageAdapter", () => {
  it("keeps the collapsed tool row limited to the tool name", () => {
    render(<ToolMessageAdapter call={call} result='{"ok":true,"result":"hello"}' />);

    expect(screen.getByRole("button", { name: "Read" })).toBeInTheDocument();
    expect(screen.queryByText("Completed")).not.toBeInTheDocument();
    expect(screen.queryByText("today.md")).not.toBeInTheDocument();
  });

  it("shows running tool details as soon as the call exists", () => {
    render(<ToolMessageAdapter call={call} />);

    fireEvent.click(screen.getByRole("button", { name: "Read" }));

    expect(screen.getByText("Parameters")).toBeInTheDocument();
    expect(screen.getByText("Running…")).toBeInTheDocument();
  });
});

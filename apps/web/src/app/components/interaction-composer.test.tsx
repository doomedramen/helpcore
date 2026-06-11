import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { PendingInteraction } from "@/lib/types";
import InteractionComposer from "./interaction-composer";

function interaction(question_type: "single_select" | "multi_select" | "text"): PendingInteraction {
  return {
    id: "interaction-1",
    conversation_id: "conversation-1",
    message_id: "message-1",
    tool_call_id: "tool-1",
    created_at: "2026-06-11T00:00:00Z",
    kind: "questions",
    questions: [
      {
        id: "answer",
        header: "Decision",
        question: "How should this work?",
        question_type,
        options:
          question_type === "text"
            ? []
            : [
                {
                  id: "fast",
                  label: "Fast",
                  description: "Prefer speed over additional checks.",
                },
                {
                  id: "careful",
                  label: "Careful",
                  description: "Run additional checks before continuing.",
                },
              ],
      },
    ],
  };
}

function renderComposer(value: PendingInteraction) {
  const onSubmit = vi.fn();
  const onDismiss = vi.fn();
  render(
    <InteractionComposer
      interaction={value}
      submitting={false}
      error=""
      onSubmit={onSubmit}
      onDismiss={onDismiss}
    />,
  );
  return { onSubmit, onDismiss };
}

describe("InteractionComposer", () => {
  it("shows required option help and submits a single selection", async () => {
    const { onSubmit } = renderComposer(interaction("single_select"));
    const help = screen.getByRole("button", {
      name: "More information about Careful",
    });
    fireEvent.focus(help);
    expect(await screen.findByText("Run additional checks before continuing.")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "2" });
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(onSubmit).toHaveBeenCalledWith({
      kind: "questions",
      answers: [
        {
          question_id: "answer",
          option_ids: ["careful"],
          custom_response: null,
        },
      ],
    });
  });

  it("submits immediately when clicking a single-select option", () => {
    const { onSubmit } = renderComposer(interaction("single_select"));
    fireEvent.click(screen.getByRole("button", { name: "Careful" }));
    expect(onSubmit).toHaveBeenCalledWith({
      kind: "questions",
      answers: [
        {
          question_id: "answer",
          option_ids: ["careful"],
          custom_response: null,
        },
      ],
    });
  });

  it("supports multiple choices plus a custom response", () => {
    const { onSubmit } = renderComposer(interaction("multi_select"));
    fireEvent.click(screen.getByRole("button", { name: "Fast" }));
    fireEvent.click(screen.getByRole("button", { name: "Careful" }));
    fireEvent.change(
      screen.getByRole("textbox", { name: "Custom answer for How should this work?" }),
      {
        target: { value: "Also keep an audit trail" },
      },
    );
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));

    expect(onSubmit).toHaveBeenCalledWith({
      kind: "questions",
      answers: [
        {
          question_id: "answer",
          option_ids: ["fast", "careful"],
          custom_response: "Also keep an audit trail",
        },
      ],
    });
  });

  it("supports simple text questions and Escape dismissal", async () => {
    const { onSubmit, onDismiss } = renderComposer(interaction("text"));
    const answer = screen.getByPlaceholderText("Type your answer");
    fireEvent.change(answer, { target: { value: "Keep the answer short" } });
    fireEvent.keyDown(answer, { key: "Enter" });
    expect(onSubmit).toHaveBeenCalledWith({
      kind: "questions",
      answers: [
        {
          question_id: "answer",
          option_ids: [],
          custom_response: "Keep the answer short",
        },
      ],
    });

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(onDismiss).toHaveBeenCalled());
  });
});

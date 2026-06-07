import { CheckCircle2, CircleAlert } from "lucide-react";

interface StatusMessageProps {
  type: "error" | "success";
  message: string;
}

export default function StatusMessage({ type, message }: StatusMessageProps) {
  const isError = type === "error";
  return (
    <div
      className={`flex items-start gap-2.5 rounded-xl border px-4 py-3 text-sm ${
        isError
          ? "border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300"
          : "border-teal-200 bg-teal-50 text-teal-700 dark:border-teal-900 dark:bg-teal-950/50 dark:text-teal-300"
      }`}
    >
      {isError ? (
        <CircleAlert size={16} className="mt-0.5 shrink-0" />
      ) : (
        <CheckCircle2 size={16} className="mt-0.5 shrink-0" />
      )}
      <span>{message}</span>
    </div>
  );
}

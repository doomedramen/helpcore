"use client";

import { usePromptInputAttachments } from "@/components/ai-elements/prompt-input";
import {
  Attachments,
  Attachment,
  AttachmentPreview,
  AttachmentInfo,
  AttachmentRemove,
} from "@/components/ai-elements/attachments";

export default function PromptAttachments() {
  const { files, remove } = usePromptInputAttachments();

  if (files.length === 0) return null;

  return (
    <Attachments variant="inline" className="px-2">
      {files.map((file) => (
        <Attachment key={file.id} data={file} onRemove={() => remove(file.id)}>
          <AttachmentPreview />
          <AttachmentInfo />
          <AttachmentRemove />
        </Attachment>
      ))}
    </Attachments>
  );
}

import { useCallback, useEffect, useRef, useState, type ChangeEvent } from "react";
import { api, errorMessage, type Attachment } from "../api";
import {
  ATTACHMENT_ACCEPT,
  MAX_ENTRY_ATTACHMENTS,
  fileProblem,
  formatSize,
} from "../lib/attachments";
import { Button } from "./Button";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";
import { Spinner } from "./Spinner";
import { useT } from "../i18n";

interface AttachmentsPanelProps {
  companyId: string;
  entryId: string;
  canWrite: boolean;
}

/**
 * The files attached to an entry: the invoice, the receipt (ADR 0036). Each is downloaded from a plain
 * link, so the browser saves it rather than showing it as a page of the app.
 */
export function AttachmentsPanel({ companyId, entryId, canWrite }: AttachmentsPanelProps) {
  const t = useT();
  const [attachments, setAttachments] = useState<Attachment[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const picker = useRef<HTMLInputElement>(null);

  /** Reads the list; says whether it could. */
  const load = useCallback(async () => {
    try {
      setAttachments((await api.listAttachments(companyId, entryId)).items);
      setError(null);
      return true;
    } catch (failure) {
      setError(errorMessage(failure));
      return false;
    }
  }, [companyId, entryId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function attach(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (file === undefined || sending || removingId !== null) {
      return;
    }
    const problem = fileProblem(file, t);
    if (problem !== null) {
      setError(problem);
      setSaved(null);
      return;
    }
    setSending(true);
    setError(null);
    setSaved(null);
    try {
      const made = await api.attachFile(companyId, entryId, file);
      if (await load()) {
        setSaved(t("{name} attached.", { name: made.file_name }));
      }
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSending(false);
    }
  }

  async function remove(attachment: Attachment) {
    if (removingId !== null) {
      return;
    }
    setRemovingId(attachment.id);
    setError(null);
    setSaved(null);
    try {
      await api.removeAttachment(companyId, entryId, attachment.id);
      await load();
    } catch (failure) {
      // Another tab may have removed it already: read what is true now, then say what happened (the
      // read clears the message).
      await load();
      setError(errorMessage(failure));
    } finally {
      setRemovingId(null);
    }
  }

  const full = attachments !== null && attachments.length >= MAX_ENTRY_ATTACHMENTS;

  return (
    <div className="attachments">
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      {attachments === null && error === null ? (
        <p role="status" className="status">
          <Spinner /> {t("Loading…")}
        </p>
      ) : attachments === null ? (
        <Button type="button" variant="ghost" onClick={() => void load()}>
          {t("Try again")}
        </Button>
      ) : (
        attachments !== null &&
        (attachments.length === 0 ? (
          <p className="muted">{t("No file attached.")}</p>
        ) : (
          <ul className="history">
            {attachments.map((attachment) => (
              <li key={attachment.id}>
                <a href={api.attachmentUrl(companyId, entryId, attachment.id)} download>
                  {attachment.file_name}
                </a>{" "}
                <span className="muted">{formatSize(attachment.size)}</span>
                {canWrite && (
                  <Button
                    type="button"
                    variant="ghost"
                    className="btn-small"
                    aria-label={t("Remove {name}", { name: attachment.file_name })}
                    pending={removingId === attachment.id}
                    disabled={removingId !== null || sending}
                    onClick={() => void remove(attachment)}
                  >
                    {t("Remove")}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        ))
      )}
      {canWrite && (
        <>
          <input
            ref={picker}
            type="file"
            hidden
            accept={ATTACHMENT_ACCEPT}
            aria-label={t("File")}
            onChange={(event) => void attach(event)}
          />
          <Button
            type="button"
            icon={PlusIcon}
            pending={sending}
            disabled={full || removingId !== null}
            onClick={() => picker.current?.click()}
          >
            {t("Attach file")}
          </Button>
          <p className="muted">
            {full
              ? t("This entry already has {max} files: remove one to attach another.", { max: MAX_ENTRY_ATTACHMENTS })
              : t("Up to {max} files of 10 MB: PDF, image, text, CSV or XML.", { max: MAX_ENTRY_ATTACHMENTS })}
          </p>
        </>
      )}
    </div>
  );
}

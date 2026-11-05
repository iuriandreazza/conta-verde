import { AnimatePresence, motion } from "motion/react";
import { BadgeAlertIcon } from "./icons/badge-alert";
import { CheckIcon } from "./icons/check";

export type NoticeTone = "error" | "warning" | "success";

/**
 * A message that slides in and out instead of making the layout jump. Errors are announced
 * assertively (`role="alert"`); warnings and confirmations politely (`role="status"`).
 */
export function Notice({
  message,
  tone = "error",
}: {
  message: string | null;
  tone?: NoticeTone;
}) {
  const Icon = tone === "success" ? CheckIcon : BadgeAlertIcon;
  return (
    <AnimatePresence initial={false}>
      {message && (
        // A div, not a <p>: the icon component renders a <div>, which may not sit inside a paragraph.
        <motion.div
          key={message}
          role={tone === "error" ? "alert" : "status"}
          className={`notice notice-${tone}`}
          initial={{ opacity: 0, y: -6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -6 }}
          transition={{ duration: 0.18 }}
        >
          <Icon size={18} aria-hidden />
          <span>{message}</span>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

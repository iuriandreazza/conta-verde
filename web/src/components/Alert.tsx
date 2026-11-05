import { AnimatePresence, motion } from "motion/react";
import { BadgeAlertIcon } from "./icons/badge-alert";

/** Error line that slides in and out instead of making the layout jump. */
export function Alert({ message }: { message: string | null }) {
  return (
    <AnimatePresence initial={false}>
      {message && (
        // A div, not a <p>: the icon component renders a <div>, which may not sit inside a paragraph.
        <motion.div
          key={message}
          role="alert"
          className="alert"
          initial={{ opacity: 0, y: -6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -6 }}
          transition={{ duration: 0.18 }}
        >
          <BadgeAlertIcon size={18} aria-hidden />
          <span>{message}</span>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

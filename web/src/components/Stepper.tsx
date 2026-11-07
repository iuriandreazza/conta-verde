import { CheckIcon } from "./icons/check";

/** The steps of a flow that goes in order: the ones done are ticked, the one the person is on is marked. */
export function Stepper({ steps, current }: { steps: string[]; current: number }) {
  return (
    <ol className="stepper" aria-label="Etapas">
      {steps.map((label, index) => (
        <li
          key={label}
          data-state={index < current ? "done" : index === current ? "current" : "todo"}
          aria-current={index === current ? "step" : undefined}
        >
          <span className="stepper-dot" aria-hidden>
            {index < current ? <CheckIcon size={14} /> : index + 1}
          </span>
          <span className="stepper-label">{label}</span>
        </li>
      ))}
    </ol>
  );
}

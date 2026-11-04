// Field: a labelled text control. Wires the label to the control, points
// `aria-describedby` at the hint and error ids, and flips `aria-invalid` on the
// control when an error message is present. The control itself is TextInput, so
// Field and TextInput share one surface treatment.
import { useId, type ReactNode } from "react";
import { cn } from "../../lib/cn";
import { TextInput, type TextInputProps } from "./TextInput";

export interface FieldProps extends Omit<TextInputProps, "id" | "invalid"> {
  label: string;
  /** Helper text below the control (e.g. "applies on next capture start"). */
  hint?: ReactNode;
  /** Error message; sets aria-invalid and the danger border. */
  error?: string | null;
  /** Optional explicit id; generated when omitted. */
  id?: string;
}

export function Field({
  label,
  hint,
  error,
  id: idProp,
  required = false,
  disabled = false,
  className,
  ...rest
}: FieldProps) {
  const generatedId = useId();
  const id = idProp ?? generatedId;
  const hintId = `${id}-hint`;
  const errorId = `${id}-error`;
  const invalid = Boolean(error);
  const describedBy =
    [hint ? hintId : null, error ? errorId : null].filter(Boolean).join(" ") ||
    undefined;

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <label
        htmlFor={id}
        className={cn(
          "text-t-ui font-medium",
          disabled ? "text-fg-disabled" : "text-fg-primary",
        )}
      >
        {label}
        {required && (
          <span aria-hidden="true" className="text-status-danger-fg">
            {" *"}
          </span>
        )}
      </label>
      <TextInput
        {...rest}
        id={id}
        required={required}
        disabled={disabled}
        invalid={invalid}
        aria-describedby={describedBy}
      />
      {hint && (
        <span
          id={hintId}
          className={cn(
            "text-t-xs",
            disabled ? "text-fg-disabled" : "text-fg-tertiary",
          )}
        >
          {hint}
        </span>
      )}
      {error && (
        <span id={errorId} role="alert" className="text-t-xs text-status-danger-fg">
          {error}
        </span>
      )}
    </div>
  );
}

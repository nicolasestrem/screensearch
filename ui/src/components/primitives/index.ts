// Barrel for the UI primitives (UI_REFERENCE §5, v2 rebuild).
// Import from here: import { Button, Panel, Chip } from "../primitives";
// v1 export names and props stay compatible (domain/**, shell/**, overlay/**,
// routes/Settings.tsx consume them; P10 deletes the legacy tokens shim).
export {
  Button,
  type ButtonProps,
  type ButtonVariant,
  type ButtonSize,
} from "./Button";
export { IconButton, type IconButtonProps } from "./IconButton";
export { Panel, type PanelProps } from "./Panel";
export { Expander, type ExpanderProps, type ExpanderPreview } from "./Expander";
export { Chip, type ChipProps, type ChipTone } from "./Chip";
export { Field, type FieldProps } from "./Field";
export { TextInput, type TextInputProps } from "./TextInput";
export { Textarea, type TextareaProps } from "./Textarea";
export { Select, type SelectProps, type SelectOption } from "./Select";
export { Combobox, type ComboboxProps, type ComboboxOption } from "./Combobox";
export { Toggle, type ToggleProps } from "./Toggle";
export { Checkbox, type CheckboxProps } from "./Checkbox";
export { RadioGroup, type RadioGroupProps, type RadioOption } from "./RadioGroup";
export { Slider, type SliderProps, type SliderState, type SliderPreview } from "./Slider";
export { Segmented, type SegmentedProps, type SegmentedOption } from "./Segmented";
export { Tabs, type TabsProps, type TabItem } from "./Tabs";
export { Skeleton, type SkeletonProps } from "./Skeleton";
export { Spinner, type SpinnerProps } from "./Spinner";
export { EmptyState, type EmptyStateProps } from "./EmptyState";
export { ErrorState, type ErrorStateProps } from "./ErrorState";
export { Table, type TableProps, type TableColumn, type TableSort, type TableState } from "./Table";
export { ListRow, type ListRowProps, type ListRowState, type ListRowPreview } from "./ListRow";
export { Tooltip, type TooltipProps, type TooltipSide } from "./Tooltip";
export { Popover, type PopoverProps, type PopoverTriggerProps, type PopoverSide, type PopoverAlign } from "./Popover";
export { Dialog, type DialogProps } from "./Dialog";
export { Sheet, type SheetProps, type SheetSide } from "./Sheet";
export { Badge, type BadgeProps, type BadgeTone } from "./Badge";
export { Kbd, type KbdProps } from "./Kbd";
export { Divider, type DividerProps } from "./Divider";
export { Toast, ToastViewport, type ToastProps, type ToastAction } from "./Toast";
export { useToastStore, type ToastItem } from "../../state/toastStore";
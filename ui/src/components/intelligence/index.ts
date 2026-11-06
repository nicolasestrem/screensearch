// Shared intelligence pieces (`03 §7c` P9 block): the evidence destination
// mapping, the entity/connection/insight/scope surfaces, the two override
// confirmations the Work list and the Thread page both offer, and the coarse
// readings they all format with. The routes compose these and own their own
// reads; nothing here fetches.
export { EvidenceLink, evidenceHref, type EvidenceLinkProps } from "./evidenceLink";
export { EntityChips, type EntityChipItem, type EntityChipsProps } from "./EntityChips";
export {
  ConnectionsPanel,
  type ConnectionsPanelProps,
  type ConnectionsState,
} from "./ConnectionsPanel";
export { InsightCard, type InsightCardProps } from "./InsightCard";
export { ScopeChips, type ScopeChipsProps, type ScopeEntity } from "./ScopeChips";
export {
  MergeThreadDialog,
  SplitThreadDialog,
  type MergeThreadDialogProps,
  type SplitThreadDialogProps,
  type OverrideListState,
  type ThreadOverrideSource,
  type ThreadOverrideSurvivor,
} from "./threadOverrides";
export { formatCount, formatDuration, formatGap } from "./format";

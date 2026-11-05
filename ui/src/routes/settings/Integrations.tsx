// Settings · Integrations (`UI_REFERENCE §4`, `03 §7c` / `§8`): the four `api.*` UI
// keys. The switch, bind address and port autosave through `set_settings`, which
// validates the address and hot-applies the listener (`src-tauri/src/lib.rs`
// 1238-1311); `set_api_config` is never called from here. Live state comes from
// `get_api_status` / `api_status_changed`: enabled && running is listening on the
// actual address and port, enabled && !running is either binding or the loud bind
// failure (last_error), and !enabled is "API disabled — nothing is listening".
// An invalid literal or an out-of-range port fails here, before the write, and
// keeps the typed value: no silent clamp, no save the user has to discover failed.
import { useState, type KeyboardEvent, type ReactNode } from "react";

import { Button, Field, Toggle } from "../../components/primitives";
import { useApiStatus, useSettings } from "../../lib/ipc/queries";
import { ApiCredentialRow } from "./ApiCredentialRow";
import { CategoryState } from "./CategoryState";
import { isIpLiteral } from "./ipLiteral";
import { SettingRow } from "./SettingRow";
import { useSettingField } from "./useSettingField";

export function IntegrationsCategory() {
  const settings = useSettings();
  const apiStatus = useApiStatus();
  const enabled = useSettingField<"api_enabled">("api.enabled");
  const port = useSettingField<"api_port">("api.port");
  const address = useSettingField<"api_bind_address">("api.bind_address");

  // A draft exists only while a row holds an uncommitted edit, so a refetch never
  // overwrites typing and a committed (or failed) value shows through again.
  const [portDraft, setPortDraft] = useState<string | null>(null);
  const [portError, setPortError] = useState<string | null>(null);
  const [addressDraft, setAddressDraft] = useState<string | null>(null);
  const [addressError, setAddressError] = useState<string | null>(null);

  const minPort = port.def.range?.min ?? 1_024;
  const maxPort = port.def.range?.max ?? 65_535;
  const portValue = typeof port.value === "number" ? port.value : undefined;
  const addressValue = typeof address.value === "string" ? address.value : undefined;

  const reload = (): void => {
    settings.refetch();
    apiStatus.refetch();
  };

  const commitOnEnter =
    (commit: () => void) => (event: KeyboardEvent<HTMLInputElement>) => {
      if (event.key !== "Enter") return;
      event.preventDefault();
      commit();
    };

  const commitPort = (): void => {
    if (portDraft === null) return;
    const raw = portDraft.trim();
    const next = Number(raw);
    if (raw.length === 0 || !Number.isInteger(next)) {
      setPortError(`Write the port as a whole number from ${minPort} to ${maxPort}.`);
      return;
    }
    if (next < minPort || next > maxPort) {
      setPortError(`Ports run from ${minPort} to ${maxPort}.`);
      return;
    }
    setPortError(null);
    setPortDraft(null);
    if (next !== portValue) port.commit(next);
  };

  const commitAddress = (): void => {
    if (addressDraft === null) return;
    const next = addressDraft.trim();
    if (next.length === 0) {
      setAddressError("Write the address the API should listen on.");
      return;
    }
    if (!isIpLiteral(next)) {
      setAddressError(
        "That isn’t an IPv4 or IPv6 address. Write the literal alone, such as 127.0.0.1 or ::1.",
      );
      return;
    }
    setAddressError(null);
    setAddressDraft(null);
    if (next !== addressValue) address.commit(next);
  };

  if (settings.isPending) {
    return <CategoryState category="integrations" state="loading" rows={4} />;
  }
  if (
    settings.isError ||
    enabled.value === undefined ||
    portValue === undefined ||
    addressValue === undefined
  ) {
    return (
      <CategoryState
        category="integrations"
        state="error"
        error={{
          title: "Couldn’t load the API settings",
          message:
            "The saved values couldn’t be read, so no row can show what it currently is. Nothing was changed.",
          onRetry: reload,
        }}
      />
    );
  }

  const api = apiStatus.data;
  const notListening = api !== undefined && api.enabled && !api.running;
  const frameState: "partial" | "populated" = apiStatus.isError || notListening ? "partial" : "populated";
  const partialNote = apiStatus.isError
    ? "The listener state couldn’t be read, so the live status below is unknown; every row still saves."
    : api?.last_error
      ? `The API is enabled but not listening: ${api.last_error}`
      : "The API is enabled and the listener is still binding.";

  let listener: ReactNode;
  if (apiStatus.isPending) {
    listener = "Reading the listener state…";
  } else if (api === undefined) {
    listener = (
      <span className="flex min-w-0 flex-wrap items-center gap-2">
        <span className="text-status-warn-fg">
          The listener state couldn’t be read.
        </span>
        <Button size="sm" variant="ghost" onClick={reload}>
          Retry
        </Button>
      </span>
    );
  } else if (!api.enabled) {
    listener = "API disabled — nothing is listening.";
  } else if (api.running) {
    // A wildcard listener is a real address and a poor client destination; say both.
    const socket = `${
      api.bind_address.includes(":") ? `[${api.bind_address}]` : api.bind_address
    }:${api.port}`;
    const wildcard = api.bind_address === "0.0.0.0" || api.bind_address === "::";
    listener = (
      <span className="flex min-w-0 flex-col gap-1">
        <span>
          Listening on{" "}
          <span className="break-all font-mono text-t-data text-fg-primary">
            {socket}
          </span>
        </span>
        {wildcard ? (
          <span>
            A wildcard address listens on every interface, so it does not keep the
            API on this network. Clients use this computer’s own address and port{" "}
            {api.port}.
          </span>
        ) : null}
      </span>
    );
  } else if (api.last_error) {
    listener = (
      <span className="text-status-warn-fg">
        Not listening — {api.last_error}
      </span>
    );
  } else {
    listener = "Starting — binding the listener…";
  }

  return (
    <CategoryState
      category="integrations"
      state={frameState}
      partialNote={partialNote}
    >
      {/* The `empty` cell of `UI_REFERENCE §4` for this category: API off, worded as
          that table words it. It stays inside the frame because the four rows are
          the only controls — hiding them would dead-end the off state and would
          split the read credential away from `api.token`. */}
      {api !== undefined && !api.enabled ? (
        <p
          role="status"
          data-state="empty"
          className="mb-3 min-w-0 border-l-2 border-line-strong bg-surface-inset px-3 py-2 text-t-sm text-fg-secondary"
        >
          API disabled — nothing is listening. These settings save either way; the
          address and port apply the moment you turn the API on.
        </p>
      ) : null}

      <SettingRow
        settingKey="api.enabled"
        labelInControl
        changed={enabled.changed}
        onReset={() => enabled.reset()}
        saving={enabled.saving}
        error={enabled.error}
        onRetry={enabled.retry}
        status={listener}
        control={
          <Toggle
            label={enabled.def.label}
            hint={enabled.def.effect}
            checked={enabled.value === true}
            onChange={(next) => enabled.commit(next)}
          />
        }
      >
        {api !== undefined && api.enabled && !api.running ? (
          <p
            role="status"
            className="mt-2 min-w-0 border-l-2 border-status-warn-border bg-status-warn-bg px-3 py-2 text-t-sm text-status-warn-fg"
          >
            {api.last_error
              ? `The API is on but not listening: ${api.last_error}. Change the port or the bind address below and save it again.`
              : "The API is on and the listener is still binding."}
          </p>
        ) : null}
      </SettingRow>

      <SettingRow
        settingKey="api.bind_address"
        labelInControl
        changed={address.changed}
        onReset={() => {
          setAddressDraft(null);
          setAddressError(null);
          address.reset();
        }}
        saving={address.saving}
        error={address.error}
        onRetry={address.retry}
        control={
          <Field
            label={address.def.label}
            hint={address.def.effect}
            value={addressDraft ?? addressValue}
            error={addressError}
            onChange={(event) => {
              setAddressDraft(event.currentTarget.value);
              setAddressError(null);
            }}
            onBlur={commitAddress}
            onKeyDown={commitOnEnter(commitAddress)}
            spellCheck={false}
            autoComplete="off"
            autoCapitalize="none"
            className="w-full md:w-72"
          />
        }
      >
        <p className="text-t-sm text-fg-tertiary">
          A literal address only: 127.0.0.1 or ::1 keep the API on this computer;
          0.0.0.0 or :: accept connections from the network. No port, no brackets.</p>
      </SettingRow>

      <SettingRow
        settingKey="api.port"
        labelInControl
        changed={port.changed}
        onReset={() => {
          setPortDraft(null);
          setPortError(null);
          port.reset();
        }}
        saving={port.saving}
        error={port.error}
        onRetry={port.retry}
        control={
          <Field
            label={port.def.label}
            hint={port.def.effect}
            type="number"
            inputMode="numeric"
            min={minPort}
            max={maxPort}
            value={portDraft ?? String(portValue)}
            error={portError}
            onChange={(event) => {
              setPortDraft(event.currentTarget.value);
              setPortError(null);
            }}
            onBlur={commitPort}
            onKeyDown={commitOnEnter(commitPort)}
            className="w-full md:w-48"
          />
        }
      >
        <p className="text-t-sm text-fg-tertiary">
          Ports {minPort}–{maxPort}. Changing it restarts an enabled listener
          immediately.</p>
      </SettingRow>

      <SettingRow settingKey="api.token" control={<ApiCredentialRow apiStatus={api} />} />
    </CategoryState>
  );
}

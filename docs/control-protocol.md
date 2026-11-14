# Control protocol

What a frontend or automation can ask of a running Pi Studio Dashboard. The methods are implemented in
`crates/shell/src/control.rs` (`handle`), which has no transport: the desktop app feeds it from the
webview and from a loopback socket, and tests and the `view` example call it directly.

## From outside: the loopback socket

Start the app with a control port:

```sh
cargo run -p pistudiodashboard -- --control 7878 --control-token-file control.token
```

- `--control <PORT>` (or `PISTUDIODASHBOARD_CONTROL_PORT`): serve on `127.0.0.1:<PORT>`; `0` picks any free
  port. The server binds the loopback interface only.
- `--control-token-file <FILE>` (or `PISTUDIODASHBOARD_CONTROL_TOKEN_FILE`): the file holding the token, 64
  hexadecimal characters. It is created with a fresh token when missing, readable by its owner
  only. Without this option a token for the launch is printed on standard error.

The wire format is JSON lines over TCP: one request per line, one reply per line.

```text
{"id": 1, "method": "auth", "params": {"token": "<64 hex>"}}
{"id": 1, "ok": true, "result": {"authenticated": true}}
{"id": 2, "method": "engine.execute", "params": {"command": "view.setTheme", "params": {"theme": "light"}}}
{"id": 2, "ok": true, "result": null}
```

- The first line on a connection must be `auth`. Anything else, or a wrong token, gets
  `{"ok": false, "error": "authentication required"}` and the connection is closed.
- A reply carries the request's `id` and either `"ok": true` with `result`, or `"ok": false` with
  `error`.
- A request line is at most 1 MiB. A longer one is an error.
- A request the app has not answered within 60 seconds gets
  `{"ok": false, "error": "timeout: no reply in 60s"}`. It may still be running.

With a netcat that waits for replies (`nc -q 2` on most Linux distributions):

```sh
TOKEN=$(tr -d '\r\n' < control.token)
printf '%s\n' "{\"id\":\"auth\",\"method\":\"auth\",\"params\":{\"token\":\"$TOKEN\"}}" \
  '{"id":1,"method":"engine.execute","params":{"command":"view.setTheme","params":{"theme":"light"}}}' \
  '{"id":2,"method":"ui.inspect","params":{}}' | nc -q 2 127.0.0.1 7878
```

## From the webview

The frontend calls the same methods through the Tauri command `control(method, params)`, with no
token: the webview is part of the app. It has one more command, `invoke_command(command)`, for a
menu click or a shortcut. That one may open a native dialog, so it is not offered on the socket.
After any request, from either side, the app sends the webview a `view` event when the value to
draw has changed.

## Methods

| Method | Params | Does |
|---|---|---|
| `engine.execute` | `{"command": string, "params"?: object}` | Runs any command by id, engine commands and shell commands alike. The result is the command's result |
| `engine.commands` | none | Lists the engine registry and the shell commands: `id`, `label`, `menu`, `shortcut`, `internal`, `params`, `enabled`, `reason` when disabled, and `layer` (`"engine"` or `"shell"`). `internal` marks commands fed by the app itself (`feed.ingest`); they have no menu entry |
| `ui.inspect` | none | Returns the view, exactly what the frontend draws: `{"ui": …, "document": …, "menus": […], "shellCommands": […]}`. `document` is the monitor view that `document.inspect` returns (daemon chips, status chips, totals, the card window, drill-down, rail flag: see "View shape" in `swe/specs/features/agent-monitor.md`), built with the filters, grid window and rail state held in `ui`. Each menu has a `label` and `items` with `id`, `label`, `shortcut`, `enabled`, `reason` and `checked`; an `id` of the form `<command>:<value>` is a param-bound entry (`view.setTheme:light`) and is passed back unchanged. The frontend runs a shortcut only when its item is `enabled` |
| `ui.set` | see below | Changes UI state. Every field is validated before any is applied: an unknown field, a bad value or a read-only field is an error naming it, and nothing changes |

An unknown method is an error that lists the methods.

### `ui.set` fields

| Field | Accepts |
|---|---|
| `theme` | `"dark"`, `"light"` or `"system"`: the choice, never its resolution. Otherwise: `` `theme` is `dark`, `light` or `system` `` |
| `filterDaemons` | Array of at most 50 serverIds, each 1-128 characters (empty = all daemons) |
| `filterStatuses` | Array of distinct status words: `initializing`, `idle`, `running`, `error`, `closed`, `unknown` (empty = all) |
| `attentionRailCollapsed` | Boolean |
| `focusItem` | Only `null` (clear). Setting it is a later command |
| `gridWindow` | `{"start": n, "count": n}`, non-negative integers, `count` at most 1,000 |
| `dialog` | `null` (close) or a dialog object, with string ids of 1-128 characters: `{"kind": "daemonForm", "mode": "add" \| "edit", "entryId"?}`, `{"kind": "password", "entryId"}`, `{"kind": "confirmInterrupt", "serverId", "agentId"}`, `{"kind": "removeDaemon", "entryId"}`. This only opens or closes; submitting goes through commands |
| `diagnosticsTarget` | `null` or a string of 1-128 characters |
| `toasts`, `status` | Read-only: `` `toasts` is read-only `` |

### Commands

- Engine: `command.list {filter?}`, `document.inspect {filterDaemons?, filterStatuses?, gridWindow?, railCollapsed?}`
  and `feed.ingest {events}` (internal: the app's client feeds it batches of daemon events; a
  malformed batch is an error and changes nothing; the result lists `applied`, the `effects` the
  batch produced and any `skipped` ones with the reason).
- Shell (`layer: "shell"`, view state only): `view.setTheme {theme}`, `view.filterDaemons {serverIds}`,
  `view.filterStatuses {statuses}`, `view.toggleAttentionRail {}` (`Cmd+Shift+A`) and
  `view.setGridWindow {start, count}`. They validate exactly like the matching `ui.set` field; a
  failure shows up in `ui.status`.
- Agents (engine): `agent.select {serverId, agentId}` opens the drill-down (`document.drill`, in place
  of the attention rail) and `agent.deselect {}` closes it; `agent.deselect` is in the `Agent` menu
  with the shortcut `Escape` and is enabled only while a drill-down is open.
- The card window: up to 100 cards `document.cards.window` holds all of them; above that
  (`cards.virtualised`) it holds the `gridWindow` the frontend reported plus 20 cards of overscan on
  each side, at most 200, and `cards.start` is the index of `window[0]` in the ordered list. A card
  is `{serverId, agentId, daemonName, title, model, status, statusLabel, dimmed, stale, staleSince,
  attention, currentTool, lastOutputLine, usage, selected}`; there is no `dot` field (the frontend
  derives the dot from `status` + `attention`). An `unknown` status counts and filters with `idle`.

- Daemons (engine): `daemon.add {name, host, port, tls?, webUiUrl?, password?}`,
  `daemon.edit {entryId, name?, host?, port?, tls?, webUiUrl?}`, `daemon.remove {entryId}`,
  `daemon.connect {entryId}`, `daemon.disconnect {entryId}` and
  `daemon.setPassword {entryId, password, remember}`. Validation failures come back as
  `bad params: {"errors": [{"field", "code", "message"}, ...]}` with every failing field
  (`invalid_name`, `duplicate_name`, `invalid_host`, `invalid_port`, `invalid_url`,
  `invalid_password`, `too_many_daemons`, `unknown_entry`). A password is kept in the OS keychain
  when `remember` is true and the keychain works, otherwise in memory for this run only (the
  result then carries the warning `keychain_unavailable`); it is never written to the config file,
  the view or the log (`password` params of these two commands are redacted in logs).
  `daemon.connect` and `daemon.disconnect` are disabled without a live connection (the CLI).
- Shell: `daemon.detectLocal {}` adds the local pi-studio daemon found from `PI_STUDIO_HOME`,
  `PI_STUDIO_LISTEN` and `~/.pi-studio/config.json`.
- Internal (never offered to menus): `feed.ingest`, `config.saveFailed`.

## Keeping this page current

A new control method is an arm in `handle()` and a name in `METHODS`; a new `UiState` field is an
arm in `fields::apply` (`crates/shell/src/fields.rs`) and a field in `apps/pistudiodashboard/src/lib/types.ts`. Update this page in the same change.
Most needs are better served by a new command, which every frontend gets at once.

# Smoke harness: the real pi-studio daemon

Decision (user, during sprint 001–006 implementation): smoke tests run against the sibling repo
`../pi-studio` (`/home/avatsaev/DEV/avatsaev/pi-studio`). Unit and contract tests still use the vendored
fixtures and the in-repo fake daemon; the smoke pass proves the same code against the real server.
Its mock provider needs no credentials and has no auth.

## Start the daemon (once per machine session)
A dev daemon is normally already running on `127.0.0.1:6790` (check `ss -ltn | grep 6790`). If not:

```sh
cd /home/avatsaev/DEV/avatsaev/pi-studio
PI_STUDIO_HOME=/tmp/pi-studio-dev PI_STUDIO_LISTEN=127.0.0.1:6790 PI_STUDIO_MOCK_TURN_DELAY_MS=3000 \
  npm run dev:daemon        # builds the server, then serves; mock provider only, no password
```

Never edit the pi-studio repo from dashboard work. Never use `--home`/`PI_STUDIO_HOME` of a real daemon
(`~/.pi-studio`); keep to `/tmp/pi-studio-dev` and `/tmp/pi-studio-cli`.

## Make agents and dialogs (pi-studio CLI, speaks the same wire protocol)
```sh
PS="node /home/avatsaev/DEV/avatsaev/pi-studio/packages/cli/dist/cli.js -H 127.0.0.1:6790 --home /tmp/pi-studio-cli"
$PS ls                                            # list agents
timeout 8 $PS run "hello" --cwd /tmp -p mock/mock # an agent that finishes (turn delay 3 s)
timeout 8 $PS run "#ui confirm" --cwd /tmp -p mock/mock   # stays `running`, raises a confirm dialog
timeout 8 $PS run "#ui select"  --cwd /tmp -p mock/mock   # select dialog (Allow / Block)
timeout 8 $PS run "#stream fail" --cwd /tmp -p mock/mock  # agent ends in `error`
```
`run` blocks until the turn ends, so wrap it in `timeout` (the agent stays on the daemon). Other
recipes: `#ui input|editor` (hand off to the web UI), `#ui select:9`, `#ui confirm timeout=5`,
`#ui multi`, `#ui notify`, `#stream reasoning|error`, `#todo plan`. Full grammar: header of
`packages/server/src/agent/providers/mock/ui-script.ts` and `stream-script.ts`.
`$PS agent send <id> "text"` follows up; `$PS --json ls` prints the raw `list_agents_response`.

## What to smoke, by area
- **Protocol/client (sprints 001, 004):** connect to `ws://127.0.0.1:6790/ws`, read `hello`, `list_agents`,
  decode every frame lossy-safe; the vendored fixtures must still match what the daemon sends.
- **Engine/app (sprints 002, 005, 006):** run `pistudiodashboard-cli`/the app engine against the daemon,
  add it as a daemon entry (127.0.0.1:6790), watch the agent list and statuses change live, drill into
  an agent's timeline, kill the daemon process and bring it back (stale → reconnect → snapshot).
- Anything that needs a window is still limited by there being no display (see the bootstrap summary);
  use `npm run snapshot` for the frontend.

Record every smoke command and its observed result in the task summary under "Smoke against pi-studio".
If a check can't be run, say so and why; never mark it passed.

## Stage 2 and password paths
The dev daemon has no auth and mock-only providers. Password handshake and `status`/`logs` subscriptions
need the production daemon entry (`npm start` with `PI_STUDIO_PASSWORD` and a throwaway
`PI_STUDIO_HOME`); sprint 004/task-003 and sprint 010 decide how.

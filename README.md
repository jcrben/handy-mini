# handy-mini

A network-jailed build harness for [cjpais/Handy](https://github.com/cjpais/Handy/),
producing an offline-only English-dictation `.exe` for use on a locked-down
Windows workstation.

Status: **draft / work in progress.** See [HANDOFF.md](HANDOFF.md) for the
full plan and current state.

## Layout

```
handy-mini/
├── HANDOFF.md          # plan + state + open questions (read this first)
├── network-jail/       # docker-compose mitmproxy + coredns + wrappers
│   ├── docker-compose.yml
│   ├── build-jailed.bash
│   ├── run-jailed.bash
│   ├── mitmproxy/      # allowlists + JSONL host-inventory addon
│   ├── coredns/
│   └── docs/           # expected-egress.md + usage.md
├── handy/              # GITIGNORED upstream clone — re-clone with:
│                       # git clone --depth=1 git@github.com:cjpais/Handy.git handy
```

## Quick start

```bash
# bring up the jail
docker compose -f network-jail/docker-compose.yml up -d

# build (will fail until bun + bundled-model patches are in place — see HANDOFF.md)
network-jail/build-jailed.bash

# run
network-jail/run-jailed.bash --release

# inspect what was observed
cat network-jail/runtime/data/observed-hosts.jsonl | jq -c '{phase,kind,host}'
```

## Why

Goal: prove and constrain Handy's network surface so an audited build can be
carried to a work machine without admin privileges and without phoning home.
See HANDOFF.md.

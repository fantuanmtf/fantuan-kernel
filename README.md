# package - package tooling home

This branch carries the client that drives the application catalog:
`tools/appctl/` (Python stdlib only) plus its offline smoke. It is the same
client `main` uses; it is developed and reviewed here and vendored back to
`main` so catalog PRs can rely on it immediately.

```
tools/appctl/appctl.py      entry point (list/add/remove/sync/upgrade/verify/relock/menu/sbom)
tools/appctl/app_*.py       modules, each under 300 lines
tools/smoke-apps.sh         offline fixture catalog + assertions
```

Usage, the `apps.lock` schema and the GPL firewall are documented in
`apps/README.md` and `docs/APPS.md` on `main`. CI runs the appctl self-tests
(`tools/smoke-apps.sh`, `--help`, byte-compilation) on every change here.

# Contributing to the catalog

Thanks for porting an app. Keep the review small: one app per pull request.

## Checklist

- [ ] `apps/<name>/` created from `template/`.
- [ ] `manifest.toml` complete: name, version, SPDX license, upstream, rev,
      abi_min, build, description, gpl.
- [ ] `src/` is the unmodified upstream tree at `rev`.
- [ ] Every local change is a patch in `patches/` and listed in the manifest.
- [ ] `tools/appctl/appctl.py verify --name <name>` passes in the kernel
      layer (or is explicitly a `gpl = true` app discussed with the owner).
- [ ] `README.md` explains build and run; the description is one useful line.
- [ ] CI is green.

## Open questions

Open a draft pull request early when the port needs an `abi_min` bump, a new
dependency, or a licence the catalog does not carry yet. The owner decides
what `main` vendors; landing here does not enable an app in the kernel.

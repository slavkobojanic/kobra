# Upgrading Kobra

A project names the toolchain it needs in `k.toml`, and `k.lock` records the exact one that last built it
(see [The k command](../reference/cli.md#toolchains)):

```toml
[package]
name = "notes"
k = "0.1"
```

Before 1.0 every minor may break something, and each break comes with a migration: a deprecation warning
in one minor, with a fix that rewrites your code, before the removal in a later one. Moving a project to a
newer toolchain is one command, run with that toolchain:

```bash
k upgrade
```

## What `k upgrade` does

1. **It refuses uncommitted changes**, so the upgrade is one diff to review. Commit or stash first, or
   pass `--allow-dirty` (a project outside git needs it too).
2. **It moves every lockfile to the toolchain's format.** A toolchain writes a lockfile in the format it
   read it in; only `k upgrade` moves `rpc.lock`, `schema.lock`, `storage.lock`, `routes.lock` and
   `k.lock` to a newer one. A lockfile written by a newer toolchain than the one running stops it before
   anything changes (`K8202`).
3. **It raises the requirement and the lock:** each `k = "…"` the toolchain doesn't meet becomes its minor
   (an exact `=` requirement becomes its exact version), and `k.lock` records it.
4. **It applies the migrations:** the machine-applicable fixes of deprecation warnings (`K1401`), of a
   stabilized preview still listed in `preview` (`K8110`), and of the tombstones a removed form leaves for
   one minor, repeated until nothing changes, as [`k fix`](../getting-started/commands.md#k-fix) does. It
   applies no other fixes, so the diff holds only the upgrade.
5. **It prints what remains**, as `k check` would (without running the tests), and exits with `1` when
   anything does. A problem that a section below covers is printed with that section's name.

Then run `k test`, review the diff and commit it.

```bash
k upgrade --allow-dirty
k upgrade --format json
```

## Several minors at once

A removed form is recognised, with its fix, only by the minor right after the one that deprecated it. So
a project more than one minor behind is moved one minor at a time: `k upgrade --to 0.3` on a project at
0.1 first moves it to 0.2 with the newest installed 0.2 release, then to 0.3. `--to` takes a version or a
minor, which means its newest installed release. Every toolchain it needs must be installed before
anything changes; when one isn't, `k upgrade` names it and stops.

```bash
k upgrade --to 0.3
```

If a step leaves problems it can't fix, `k upgrade` stops there: fix them, commit, and run it again to go
on.

`k upgrade` never downloads anything: install each toolchain it names first, then run it again.

```bash
k toolchain install 0.2
```

## Deploying an upgrade

Two adjacent minors always understand each other on the wire (rpcs, server streams, service tokens, the
session cookie, queued jobs), so the services of a [workspace](workspaces.md) can be deployed one at a
time across an upgrade, and open tabs keep working. Toolchains two minors apart may not: deploy each minor
in turn rather than skipping one in a rolling deploy. `k upgrade` says so when it crosses more than one
minor, and the [changelog](../reference/changelog.md) says when a minor starts writing a new format.

## What `k upgrade` can't fix

A change without a machine-applicable fix (its migration needs a decision only you can make) has a
section in this guide, listed under its minor below, and its changelog entry links to it.

## Every minor's migrations

Each minor's breaking changes and deprecations, newest first, with the code whose fix `k upgrade` applies
or the section that describes the change by hand. They are compiled from the changelog fragments when the
minor is released.

<!-- gen:upgrading -->
<!-- /gen -->

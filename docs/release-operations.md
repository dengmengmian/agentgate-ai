# Release Operations Notes

中文：发版运营与下载数据口径

This page documents how to interpret GitHub release assets for AgentGate. It is meant for maintainers looking at adoption, release quality, and user support signals.

## Download metrics

GitHub's total download count is useful as a rough activity signal, but it should not be treated as the number of real desktop installs.

AgentGate releases include several asset types:

| Asset type | Examples | How to interpret |
|---|---|---|
| Desktop installers | `.dmg`, `.exe`, `.deb`, `.AppImage` | Closest proxy for user installs. Split by OS and architecture. |
| Headless CLI builds | `agentgate-serve-*.tar.gz`, `agentgate-serve-*.zip` | Server / Docker / advanced-user interest. Track separately from desktop installs. |
| Updater metadata | `latest.json` | In-app update checks and updater clients. Do not count as installs. |
| Signatures | `.sig` | Integrity metadata. Do not count as installs. |
| Source archives | GitHub auto-generated `.zip` / `.tar.gz` | Developer interest. Not an app install signal. |

Recommended reporting:

- Desktop installer downloads by platform.
- CLI downloads by platform.
- Updater metadata requests separately.
- Signature downloads excluded from adoption totals.
- Release-page traffic, stars, issues, and discussions reviewed alongside downloads.

## Release notes

The release workflow uses `scripts/extract-release-notes.mjs`.

The script prefers a curated bilingual file at:

```text
docs/release-notes/<version>.md
```

If that file does not exist, it falls back to the matching `CHANGELOG.md` section and generates bilingual section headings.

For important releases, add a curated release note before tagging so GitHub Releases has a concise English and Chinese summary.

## Maintainer checklist

Before tagging:

- Run the full local release gate. This is the only accepted pre-release entrypoint; if Docker preflight cannot run, the release check is not complete.

```bash
pnpm test:release-local
```

- Confirm `README.md` and `README_ZH.md` point to the new installer filenames after release assets are known, or keep them pointing at the latest stable release intentionally.
- Confirm `CHANGELOG.md` has a version section.
- Add `docs/release-notes/<version>.md` for important releases.
- For non-release local debugging only, `AGENTGATE_SKIP_DOCKER_PREFLIGHT=1 pnpm test:release-local` may be used to isolate frontend/unit/quickstart failures.

After publishing:

- Check that installer assets, CLI assets, signatures, and `latest.json` uploaded successfully.
- Check that the release body is bilingual.
- Watch early issues for platform-specific install failures.

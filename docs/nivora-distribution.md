# Nivora: distribution and maintenance

## Current maintenance improvements

The home screen exposes the existing diagnostic viewer, release checker and local report exporter. No diagnostic upload occurs automatically. The diagnostic badge reports an existing error record; absence of a record is not a claim of full runtime health. Home card titles and the brand fit their own bounds.

The in-app release downloader accepts only official HTTPS GitHub release asset URLs and safe APK basenames. It requires a valid SHA-256 digest from GitHub's asset metadata, checks the exact byte count and streaming hash, and downloads to a partial file before atomic publication. Failed/interrupted downloads do not become installable candidates. GitHub assets without a digest are refused for in-app installation. Android additionally checks signatures during installation; a checksum alone does not establish publisher identity. Updater progress is throttled to 10 reports per second. Worker creation failures release the busy flag.

## Architecture builds and the universal option

The existing Android APK workflow can build ARM64, ARMv7, x86 and x86_64 separately, or additionally assemble one universal APK with `universal=true`. The universal run uses Android API 28 as a common floor. It deliberately contains real native libraries for all four architectures. A single-architecture APK is still preferable when download size matters.

The assembler requires byte-identical shared code/manifests and non-engine assets. Engine packs must contain identical data after accounting for JSON key order, tag-set iteration order, lambda diagnostic addresses and ZIP metadata. Item order, registry IDs and gameplay values cannot change. Retained manifest hashes are validated and an original, unmodified resource pack is preserved. It verifies each ELF architecture, rejects duplicate/missing ABIs and real shared-payload conflicts, strips old APK signatures, and writes a new unsigned APK. The workflow aligns it, signs it and performs the same public payload audit. It records SHA-256, commit, byte count and signing mode. A conflict fails assembly rather than choosing an arbitrary shared file.

No padding, unused SDK or game resource is added to reach a size target. Universal APK size is measured after assembly; exceeding 100 MB is not a release gate or a compatibility claim. Existing wgpu, winit, Rust TLS, image decoding and SHA-256 libraries provide the actual graphics/input/network/verification functionality.

## Signing and validation

The workflow uses the repository's permanent signing secrets if configured. Without them, artifacts have a temporary development signature and still require official offline signing before use because startup certificate verification is enabled. This work does not create a new signing identity or change the expected permanent certificate. Never publish the private keystore or passwords.

Local regressions cover valid and tampered/truncated/oversized APK streams, interrupted reads, storage write failures, metadata origin/path/digest validation, universal merging, conflicting shared payloads, missing/duplicate ABIs and wrong ELF architecture. Payload audit tests also run. Full APK compilation runs in Actions. No phone benchmark, FPS gain or visual/runtime compatibility is inferred from these tests.

## Reuse successful native builds

If only universal packaging failed, dispatch with `universal=true` and `universal_source_run_id` pointing to the earlier run. The packaging job verifies the source workflow, repository and four successful architecture jobs; it checks artifact hashes and records the original source commit separately from the assembly tooling commit. Native compilation is skipped. This is not a new build of the game code.

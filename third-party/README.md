# Dependency notices

Gravity's `LICENSE` covers Gravity. The separate notice bundles preserve upstream
license, copyright and NOTICE texts for dependencies of the official macOS ARM64
app and daemon. `inventory.json` records exact versions, provenance, file hashes
and scope against the committed lockfiles. Other distribution targets need their
own review before release.

## Distribution scope

- The desktop bundle includes its production JavaScript dependency closure, its
  native dependencies and the bundled daemon's dependencies. Type-only packages
  are marked. Tree shaking can remove code from that conservative closure.
- Cargo's target-filtered dependency graph supplies native runtime and build/test
  inventories. Normal edges excluding proc macros identify runtime candidates.
  Build/test notices are also retained because macros and build scripts can emit
  code. Dependencies for other platforms are recorded but not bundled. System
  Apple frameworks are linked from macOS, not redistributed. The bundled SQLite
  and ring sources include their own upstream license material.
- The Rust toolchain's official standard-library copyright inventory and license
  texts cover statically linked standard-library/runtime code outside Cargo.lock.
  Its HTML is rendered as text while preserving notice wording. Release builds
  check the exact rustc version; a toolchain update requires regenerating notices
  with the matching official `rustc` component installed. The upstream library inventory
  also includes platform/build dependencies conservatively.
- Marketing has no third-party application imports. Vite's emitted modulepreload
  helper is covered by its upstream notice. Wrangler, workerd, sharp/libvips,
  lightningcss, TypeScript and test tools run during development/build/deployment;
  their binaries are not shipped in the website or Worker. A runtime import or
  bundler change requires reviewing this classification. The public notice is
  served at `/THIRD_PARTY_NOTICES.txt`.

## License choices and source access

All collected upstream license and NOTICE files are retained, including nested
vendored notices. Alternatives in an SPDX expression are not cumulative license
requirements. DOMPurify is used under its Apache-2.0 alternative. Permissive
alternatives apply to other dual-licensed dependencies; AND terms and bundled
third-party terms are preserved (for example ring, encoding_rs and PostHog).

The MPL-only native dependencies (cssparser, cssparser-macros, dtoa-short,
option-ext and selectors) are unmodified registry sources. Each notice identifies
the exact source archive, its lockfile SHA-256, MPL terms and the recipient's
source access. Some are build dependencies; they are covered conservatively too.
These source archives are available directly without authentication or charge.
If Gravity patches one, make the modified MPL source available and update these
instructions before distributing an executable. See Mozilla's
[MPL distribution guidance](https://www.mozilla.org/en-US/MPL/2.0/FAQ/#q8-i-want-to-distribute-outside-my-organization-executable-programs-or-libraries-that-i-have-compiled-from-someone-elses-unchanged-mpl-licensed-source-code-either-standalone-or-part-of-a-larger-work-what-do-i-have-to-do).

The PostHog core/browser-common npm archives omit license files. Their overrides
retain the complete upstream repository license at each exact package release
tag's commit, including the Apache and embedded MIT notices. The serial override
comes from the v0.4.0 commit. Other overrides use the crate's published VCS commit;
selectors also retains its source license header and Mozilla's full MPL text.
No copyright holders or years are synthesized from package author fields.
Line endings and trailing whitespace are normalized without changing notice
wording. Override records retain hashes of the original upstream bytes as well.

## Updating and checking

Use Python 3.11+, Cargo and pnpm with frozen desktop and marketing installs:

```sh
pnpm --dir apps/desktop install --frozen-lockfile
pnpm --dir apps/marketing install --frozen-lockfile
pnpm notices:generate
pnpm notices:check
```

Generation reads exact installed package sources and `cargo metadata --locked`,
and fails when included packages have no authentic license text. Review new
licenses, vendored source, generated code and distribution scopes; automated
collection is not a substitute for this review. Missing files must be retrieved
from the matching authoritative revision into `overrides`, with the URL recorded
in its index. Never substitute just an SPDX label. Commit the regenerated bundles
and inventory together. Offline CI checks detect changed locks, manifests,
generator inputs, overrides and notice bundles. Changing supported platforms or
marketing runtime imports requires updating the generator and this document.

The app carries `Contents/Resources/THIRD_PARTY_NOTICES.txt`; daemon archives
carry `THIRD_PARTY_NOTICES.txt` and Gravity's `LICENSE`. `gravityd
--third-party-notices` prints the embedded daemon notice even after the binary is
installed or copied separately. Manual release dispatches verify the packaged
copies before artifact upload; publishing still requires a release tag.

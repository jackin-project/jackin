Crypto fixtures retain authentic upstream bytes, with no Jackin-release authority claimed.

cosign-3.1.3-checksums.txt and cosign-3.1.3-checksums.bundle.json:
https://github.com/sigstore/cosign/releases/download/v3.1.3/cosign_checksums.txt
https://github.com/sigstore/cosign/releases/download/v3.1.3/cosign_checksums.txt.sigstore.json
Verified independently with Cosign. Exact email keyless@projectsigstore.iam.gserviceaccount.com,
issuer https://accounts.google.com. Standard bundle0.3, hashedrekordv1 SET,
inclusion proof/checkpoint, certificate/SCT and RFC3161 timestamp.

velnor-0.1.13-release-manifest.json and velnor-0.1.13-provenance.bundle.json:
https://github.com/tailrocks/velnor-new/releases/download/v0.1.13/release-manifest.json
https://github.com/tailrocks/velnor-new/releases/download/v0.1.13/release-manifest.json.intoto.jsonl
Exact workflow product-release-generator.yml@refs/heads/main, GitHub issuer;
source c6304405ebf3c189257cccbf114dc08dc8e1fcf4. Standard bundle0.3 DSSE,
Rekorv1 SET and inclusion proof. These authentic claims must fail Jackin policy.

jackin-policy-receipt.json: synthetic, unsigned canonical Velnor v2 receipt,
used only for fixed inventory and rejection tests. Never accepted as crypto proof.

sigstore-python-rekor2*:
https://github.com/sigstore/sigstore-python/tree/71b63df1aea910fbfe5ba0fb1dc1199a27f56b25/test/assets
Authentic message-signature and DSSE Rekor v2 bundles. The message artifact is
included; the DSSE artifact is verified by upstream's recorded SHA-256 subject
(a0cfc71271d6e278e57cd332ff957c3f7043fdda354c4cbb190a30d56efa01bf).
Distributed under Apache-2.0; unmodified upstream license: sigstore-python-LICENSE.

sigstore-rust-staging-root.json:
https://github.com/sigstore/sigstore-rust/blob/f950b1b683560aadf7737539f8d7eaa656a9f0c0/crates/sigstore-trust-root/src/trusted_root_staging.json
Distributed under Apache-2.0; unmodified upstream license: sigstore-rust-LICENSE.
This explicit staging root is test-only. Production never accepts a downloaded
asset or bundle's choice of trust root. Tests verify exact identities and issuer,
certificate/SCT, Rekor v2 proof/checkpoint, and authenticated RFC3161 timestamps.

Cosign fixtures are distributed under upstream Apache-2.0:
https://github.com/sigstore/cosign/blob/v3.1.3/LICENSE
Velnor fixtures use upstream Apache-2.0 (the MIT OR Apache-2.0 choice):
https://github.com/tailrocks/velnor-new/blob/c6304405ebf3c189257cccbf114dc08dc8e1fcf4/LICENSE-APACHE

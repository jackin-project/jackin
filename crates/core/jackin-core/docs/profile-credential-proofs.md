# Profile credential proof contract

Implementation: [profile_material_v2.rs](../src/profile_material_v2.rs).

## Profile credential proofs (V2)

The V2 helpers use a separate raw-input contract. They do not replace the V1 helpers or any live account, broker, cache, or admission fingerprint.

- `profile_credential_source_identity_v2` hashes the agent, exact provider slug, exact path bytes, and an optional raw JSON selector. An absent selector differs from the JSON value `null`. The function does not read or canonicalize the path.
- `profile_credential_material_revision_v2` hashes raw material under an agent-specific V2 domain. Other agents use strict JSON. Decoded duplicate keys reject. Object order and JSON whitespace do not affect the result; arrays keep order. Strings use their decoded value. Numbers keep their exact validated token, so `1`, `1.0`, `1e0`, `1E+0`, and `-0` differ. A valid large number does not prove that its provider accepts the value.
- Amp validates the complete JSON input before it selects one nonblank token from either supported key. It hashes the token under the canonical key. Other valid metadata does not affect the revision, but it still counts toward input limits and duplicate-key checks. Omp material is opaque binary data; the helper does not parse it as JSON.
- `profile_credential_source_material_v2` pairs an identity with a revision. `compare_profile_credential_material_proofs` compares complete proofs within one scheme. It returns `RevalidationRequired` across V1 and V2, even when the hex strings match.

Each raw selector and material input has its own limit. JSON keys count toward the byte limit but not the value count. The root, every container, and every scalar each count as one value.

| V2 input limit | Value |
|---|---:|
| Raw selector or material bytes | 8 MiB |
| JSON values, including the root | 65,536 |
| Open JSON containers | 127 |

Omp material uses the byte limit only. Its bytes do not use the JSON value or depth limits. Provider and path fields have no added V2 byte limit.

V2 identity and material wire records require exact scheme tags: `jackin.profile-credential-source.v2` and `jackin.profile-credential-material.v2`. The fingerprint fields serialize as 64 lowercase hexadecimal characters. Unknown, missing, duplicate, or wrongly typed fields reject. Deserialization checks shape only; it does not authenticate a digest or prove source custody. A caller that reads a serialized record must bound its bytes before deserialization.

Fingerprints are change and equality evidence. They do not prove authorization, credential validity, provider identity, or custody of the original bytes. The V2 helpers do not migrate V1 records or replace existing account or broker fingerprint flows.

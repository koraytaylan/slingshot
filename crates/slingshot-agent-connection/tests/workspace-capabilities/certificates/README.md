# Test provider records

Certificates used only by the capability probes in this crate. They chain to
roots created for this repository, name hosts under the reserved `.invalid`
top-level domain, and are never trusted by product code.

| File | Role |
|---|---|
| `identity-management-root.pem` | Root that only the identity-management route trusts |
| `author-root.pem` | Root that only the author route trusts |
| `client-authentication-root.pem` | Root restricted to a purpose other than server authentication |
| `name-constrained-root.pem` | Root carrying permitted and excluded name constraints and a policy identifier |
| `author-leaf.pem` | Server record for `author.example.invalid`, issued by the author root |
| `identity-management-leaf.pem` | Server record for `identity.example.invalid`, issued by the identity-management root |
| `client-only-leaf.pem` | Record issued by the author root whose purpose is not server authentication |
| `author-root-public-key.pem` | Public key of the author root, used to load a verification key |

No private key is committed. A probe that needs to sign uses a key it derives in
memory for that run.

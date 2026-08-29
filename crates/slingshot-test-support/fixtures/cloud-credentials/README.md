# Cloud service credentials

Documents shaped like the one the Adobe Experience Manager Developer Console
downloads, with real key pairs generated once and committed. The secrets are
sentinels: no real credential appears here, and every test scans its output for
them.

- `valid.json` is the documented success shape.
- `rotated-key.json` names the same principal with a different key pair.
- `other-organization.json`, `other-client.json`, and
  `other-technical-account.json` each change exactly one member of the principal
  tuple, which are the three that must move the identity.
- `reordered-metascopes.json` names the same scope in the other order;
  `repeated-metascope.json` names one twice.
- `key-mismatch.json` pairs a private key with another key's certificate.
- `deprecated-product.json` is the deprecated Adobe Developer Console
  credential, which is a different product rather than a malformed file.
- `beyond-depth-limit.json` is too deep and also shaped like that deprecated
  product, so reporting the product would prove the shape was read first.

# Token assertions

`assertion-vectors.json` pins the exact bytes one assertion has for the
credential in `../cloud-credentials/valid.json` at one fixed sampled second. The
signature scheme is deterministic, so the complete compact form is a constant
rather than something only verifiable after the fact.

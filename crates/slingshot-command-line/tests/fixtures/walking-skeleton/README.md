# Walking skeleton outputs

The hand-authored shape of everything the product executable writes to its
result stream during the walking proof, and the one diagnostic a refused
invocation writes. A value that differs between runs is replaced by the
normalization the assertion applies, so the fixture pins the shape rather than
the run: `<process-identifier>`, `<readiness-nonce>`, and `<disposition>`.

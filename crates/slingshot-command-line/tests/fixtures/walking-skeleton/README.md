# Walking skeleton outputs

Every line the product executable writes to its result stream during the
walking proof, and the one diagnostic a refused invocation writes. The lines
are the closed outcome vocabulary rendered for a person: an action and what it
found or did, and nothing that differs between runs.

A readiness nonce and a process identifier appear in none of them. A nonce
authorizes a cooperative stop, so it lives in the runtime state its daemon owns
rather than on a stream a caller may log, and the proof reads it from there.

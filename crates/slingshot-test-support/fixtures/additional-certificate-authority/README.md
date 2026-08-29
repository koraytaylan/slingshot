# Additional certificate authorities

Real certificates, generated once and committed, because a parser proved only
against bytes it also produced proves very little.

- `one-authority.pem` and `two-authorities.pem` are certificate authorities an
  operator may legitimately extend author trust to.
- `other-authority.pem` is the second of them alone, so an extension can be
  made from an authority the platform snapshot does not already hold.
- `duplicate-authority.pem` names one of them twice.
- `end-entity.pem` is a server certificate rather than an authority.
- `other-purpose.pem` is an authority whose stated purpose is not server
  authentication.
- `with-private-key.pem` carries a private key beside the certificate.
- `malformed.pem`, `empty.pem`, and `surplus-text.pem` are sources whose meaning
  would have to be guessed at.

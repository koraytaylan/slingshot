# Selected-author loopback TLS fixtures

These certificates and the **publicly committed, test-only private key** are
exclusively for local transport tests. Never use them as deployment credentials
or install their root in a real trust store.

The P-256 root signs a server-authentication leaf with only the IP subject
alternative name `127.0.0.1`. Consequently a connection naming `localhost` must
fail hostname validation even when it trusts this root. A separately selected
root must reject this leaf. The test restricts its server to TLS 1.2 or TLS 1.3
and checks the actual negotiated version through the product connector.

Generated with OpenSSL for these fixtures; certificates have a 100-year lifetime
from generation to keep runtime tests independent of short certificate expiry.
Only the leaf key is retained. The signing key is not part of the repository.

`ims-test-root.pem` and `ims-test-leaf.pem` are a separate test-only CA/leaf
chain for the exact manifest hostname `ims-na1.adobelogin.com`. These deliberately
reuse the publicly committed test key for both certificates. They establish no
real-world trust and must never be installed outside tests. The IMS client tests
substitute only TCP dialing through a compile-time test seam; TLS SNI, hostname
verification, platform-root validation, ALPN and HTTP codecs remain real. The
certificates were generated with OpenSSL with CA signing usage on the root and
serverAuth/DNS SAN usage on the leaf, with the same 100-year fixture lifetime.

`ims-ca-author-leaf.pem` is another leaf signed by that IMS fixture CA using the
same public test key, with serverAuth and only the IP SAN `127.0.0.1`. The outer
Cloud boundary test uses both valid leaves to prove that an author-only CA
extension accepts author TLS but does not authorize IMS. Cross-crate tests opt
into the `test-support` feature, whose TCP override rejects non-loopback IPs;
normal builds expose neither this constructor nor a runtime endpoint override.

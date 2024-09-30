<h3 name="header-value-encoding">Why do some HTTP headers show up mangled?</h3>

HTTP header values are officially only supposed to contain ASCII. Other bytes are "opaque data":

> Historically, HTTP has allowed field content with text in the ISO-8859-1 charset [[ISO-8859-1](https://datatracker.ietf.org/doc/html/rfc7230#ref-ISO-8859-1)], supporting other charsets only through use of [[RFC2047](https://datatracker.ietf.org/doc/html/rfc2047)] encoding.  In practice, most HTTP header field values use only a subset of the US-ASCII charset [[USASCII](https://datatracker.ietf.org/doc/html/rfc7230#ref-USASCII)].  Newly defined header fields SHOULD limit their field values to US-ASCII octets.  A recipient SHOULD treat other octets in field content (obs-text) as opaque data.

([RFC 7230](https://datatracker.ietf.org/doc/html/rfc7230#section-3.2.4))

In practice some headers are for some purposes treated like UTF-8, which supports all languages and characters in Unicode. But if you try to access header values through a browser's `fetch()` API or view them in the developer tools then they tend to be decoded as ISO-8859-1, which only supports a very limited number of characters and may not be the actual intended encoding.

xh as of version 0.23.0 shows the ISO-8859-1 decoding by default to avoid a confusing difference with web browsers. If the value looks like valid UTF-8 then it additionally shows the UTF-8 decoding.

That is, the following request:
```console
xh -v https://example.org Smile:☺
```
Displays the `Smile` header like this:
```
Smile: â�º (UTF-8: ☺)
```
The server will probably see `â�º` instead of the smiley. Or it might see `☺` after all. It depends!

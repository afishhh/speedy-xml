## speed-xml

`speedy-xml` is a simple UTF-8-only XML pull reader/writer written to be compatible with the (default mode of the) RapidXML C++ library.

It is specifically not compliant with the XML specification because RapidXML is not compliant with the specification, if RapidXML exhibits some weird behaviour `speedy-xml` should do so too.

Unlike RapidXML, this library contains only a pull-based reader that emits events, it does not parse the XML directly into a tree. This is because constructing a tree from these events is relatively trivial and some parsers may choose to work directly on the XML events for performance, easier location-tracking and/or simplicity.

## Extensions

`speedy-xml` provides some functionality that was not originaly available in RapidXML, currently this is:
- Prefixed tag names, `speedy-xml` accepts tags like `<prefix:name attr="value">`. The same applies to closing tags but *not* attributes.

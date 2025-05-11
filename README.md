## speed-xml

`speedy-xml` is a simple UTF-8-only XML pull reader/writer written to be compatible with (the default mode of) the RapidXML C++ library.

It is specifically not compliant with the XML specification because RapidXML is not compliant with the specification, if RapidXML exhibits some weird behaviour `speedy-xml` should do so too (although see prefixed name exception below).

Unlike RapidXML, this library contains only a pull-based reader that emits events, it does not parse the XML directly into a tree. This is because constructing a tree from these events is relatively trivial and some parsers may choose to work directly on the XML events for performance, easier location-tracking and/or simplicity.

## Prefixed names

`speedy-xml`, while it tries its best to be compliant, deviates from RapidXML in a few ways. One of them is the addition of prefixed names, `speedy-xml` splits names of the form `prefix:name` into prefix and name components, this means that it will reject names of the form `a:b:c`.

## Namespace support
1. The already used `roxmltree` supports namespaces, test how to detect a namespace.
2. If a namespace is detected, search `matcher` for `namespace:element` instead of `element`. 
3. The config should already allow for `"namespace:element"`, but test to make sure.

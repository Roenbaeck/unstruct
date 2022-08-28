## Performance optimization
1. Find the maximum level at which you will parse elements. 
2. Create a `Vec<bool>` sized to that level.
3. Populate the Vec and set levels on which there are elements to parse to true.
4. Replace the `recording` variable with a lookup in the Vec using the recursion `depth`.
5. If we are below the size of the Vec, **don't parse**.
6. If the lookup returns false, **don't parse**. 

## Namespace support
1. The already used `roxmltree` supports namespaces, test how to detect a namespace.
2. If a namespace is detected, search `matcher` for `namespace:element` instead of `element`. 
3. The config should already allow for `"namespace:element"`, but test to make sure.

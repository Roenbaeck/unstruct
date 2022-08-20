Shallow first searching - children() instead of decendants() in roxmltree

unstruct.parser should control the traversal, when to recurse:
- if the next directive is on a lower level, then recurse children
- if the next directive is on a higher level, then return

I added <element> into the .parser configuration. The idea is that 
after each such <element> a row is written to file.

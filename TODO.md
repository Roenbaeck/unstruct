## Performance optimization
1. Find the maximum level at which you will parse elements. 
2. Create a `Vec<bool>` sized to that level.
3. Populate the Vec and set levels on which there are elements to parse to true.
4. Replace the `recording` variable with a lookup in the Vec using the recursion `depth`.
5. If we are below the size of the Vec, **don't parse**.
6. If the lookup returns false, **don't parse**. 


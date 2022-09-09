## Handling non-values
1. The file ending with 0005 in the CDR example produce no rows.
2. Perhaps empty occurences of <ChangeOfCharCondition/> should produce rows?
3. Decide if that is desirable and if it should be controllable in the config.

# Adding metadata columns
1. Add a new option --metadata (bool).
2. When specified additional columns are added in the output.
3. Start with adding the name of the parsed file.

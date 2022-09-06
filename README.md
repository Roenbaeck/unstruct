# unstruct
Unstruct is a program that parses simple xml files into text files, suitable for bulk inserts into a 
relational database. It is still in early development. This means it is sensitive to the structure of 
the parser config file and that the input XML files need to be well formed. If not, you are likely 
to get undesired results or even program crashes.

It is written in Rust and the goal is to be more performant than loading XML into the database and 
doing the parsing there. One example of a situation where you need fast parsing of XML is when you 
have CDR (call detail record) files in XML format, since these can become abundant very quickly. 
The files currently used for testing are mockup CDR files, found in the data subfolder. 

When testing version 0.1.3 on a recent MacBook Pro, unstruct was able to parse 10 000 CDR XML files 
per second. These 10 000 XML files resulted in _one_ tab separated file. Unstruct is many to one 
if you specify a filename pattern that match several XML files.

## Configuration
This is an example parser configuration:
```
{
    <sGW-GPRS-Ascii> {
        servedIMSI = "servedIMSI"
        servedMSISDN = "servedMSISDN"
        servedIMEISV = "servedIMEISV"
        recordOpeningTime = "recordOpeningTime"
        {
            <ChangeOfCharCondition> {
                changeTime = "changeTime"
                dataVolumeGPRSUplink = "dataVolumeGPRSUplink"
                dataVolumeGPRSDownlink = "dataVolumeGPRSDownlink"
            }
        }
        duration = "duration"
        durationUnit = "duration/@unit"
    }
}
```
The brackets indicte the hiearachy in the XML file you intend to parse. The first element you are interested 
in parsing in this example  is `<sGW-GPRS-Ascii>` and it is one level below the root level of the document. 
Within this element there are some parsing directives  on the format `column_name = "xml_element_name"` or
`column_name = "xml_element_name/@optional_attribute_name"`. The `column_name` will end up as a header in 
the file with the results from the parsing. 

The elements that are specified literally: `<sGW-GPRS-Ascii>` and `<ChangeOfCharCondition>` are the ones for 
which you want new rows to be created in the result. If an element `<sGW-GPRS-Ascii>` contains two 
`<ChangeOfCharCondition>` elements, you will get two rows, where values "above" `<ChangeOfCharCondition>`
will be reused. 

If the XML file does not contain the attribute `duration/@unit` the header `durationUnit` will still be in 
the output file, but values will be empty. Look at the file `result.txt` for example output.

## Program switches

| Switch | Description |
|--------|-------------|
| `-f, --filename <pattern>` | The name of the input xml file or matching files if wildcards are used |
| `-o, --outfile <filename>` | The name of the text file into which the results of the parsing will be output |
| `-p, --parser <filename>` | The configuration file specifying the parsing rules [default: "unstruct.parser"] |
| `-q, --quiet` | If specified the program will not output any text |

## Help
Feel free to fork and help out! We need help with at least:

* Testing different XML files (very early stages of development).
* Making it more robust (there’s practically no error handling).
* Improving it in terms of performance and functionality.
* Documenting the code.

# unstruct
Unstruct is a program that parses simple xml files into text files, suitable for bulk inserts into a relational database.
It is still in early development. This means it is sensitive to the structure of the parser config file and that the 
input XML files need to be well formed. If not, you are likely to get undesired results or even program crashes.

# Configuration
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
The brackets indicte the hiearachy in the XML file you intend to parse. The first element you are interested in parsing in this example 
is `<sGW-GPRS-Ascii>` and it is one level below the root level of the document. Within this element there are some parsing directives 
on the format `column_name = "xml_element_name"` or `column_name = "xml_element_name/@optional_attribute_name"`. The `column_name` will end up as a header in the file with 
the results from the parsing. If `xml_element_name` is found...

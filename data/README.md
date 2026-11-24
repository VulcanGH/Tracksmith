# Language code data

`iso639-2.txt` contains the bibliographic, terminological and two-letter code columns of the
[Library of Congress ISO 639-2 code list](https://www.loc.gov/standards/iso639-2/ISO-639-2_utf-8.txt),
retrieved 2026-11-24. Human-readable language names are omitted. This is factual standards data;
source attribution is retained here. The first column is the code used for Matroska's legacy field.

BCP 47 tags without a corresponding legacy code retain their exact canonical IETF representation
and use `und` for the legacy field; the change plan displays an explicit compatibility notice.

`language-names.json` derives the primary English language names from that same source for rule
template variables. These are names for reported or user-configured language tags, not language detection.

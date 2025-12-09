from lxml import etree, html


def spec():
    return {
        "name": "XmlSelect",
        "title": "XML Select",
        "category": "Data",
        "description": "Selects nodes from XML or HTML content using an XPath expression.",
        "inputs": [
            {
                "name": "input",
                "type": "string",
                "ui": "textarea",
                "required": True,
                "description": "XML or HTML content to query.",
            },
            {
                "name": "query",
                "type": "string",
                "required": True,
                "description": "XPath expression (e.g. '//div[@class=\"comment\"]').",
            },
            {
                "name": "strict",
                "type": "boolean",
                "ui": "checkbox",
                "default": False,
                "description": "Strict XML parsing. When off, uses lenient HTML parsing.",
            },
        ],
        "outputs": [
            {
                "name": "matches",
                "type": "list",
                "description": "List of text content from matching nodes.",
            },
            {
                "name": "count",
                "type": "integer",
                "description": "Number of matches found.",
            },
        ],
    }


def execute(inputs):
    content = inputs.get("input", "")
    query = inputs.get("query", "")
    strict = inputs.get("strict", False)

    if not content or not query:
        return {"matches": [], "count": 0}

    if strict:
        doc = etree.fromstring(content.encode("utf-8"))
    else:
        doc = html.fromstring(content)

    nodes = doc.xpath(query)

    matches = []
    for node in nodes:
        if isinstance(node, str):
            matches.append(node)
        elif hasattr(node, "text_content"):
            matches.append(node.text_content().strip())
        else:
            matches.append(str(node))

    return {"matches": matches, "count": len(matches)}

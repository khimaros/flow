import re
import urllib.request
import xml.etree.ElementTree as ET
from datetime import datetime, timedelta, timezone
from email.utils import parsedate_to_datetime

# global constants
DEFAULT_TIMEOUT = 30
USER_AGENT = "flow-fetch-rss/1.0"
ATOM_NS = "{http://www.w3.org/2005/Atom}"

# approximate durations for relative date parsing
RELATIVE_UNITS = {
    "second": 1,
    "minute": 60,
    "hour": 3600,
    "day": 86400,
    "week": 604800,
    "month": 2592000,  # 30 days
    "year": 31536000,  # 365 days
}
RELATIVE_RE = re.compile(
    r"^\s*(\d+)\s+(second|minute|hour|day|week|month|year)s?\s+ago\s*$",
    re.IGNORECASE,
)


def spec():
    return {
        "name": "FetchRSS",
        "title": "Fetch RSS Feed",
        "category": "Network",
        "description": "Fetch an RSS/Atom feed, optionally filter by date, return items and markdown digest",
        "inputs": [
            {
                "name": "url",
                "type": "string",
                "required": True,
                "description": "RSS/Atom feed URL (use List + Loop to fetch multiple)",
            },
            {
                "name": "from_date",
                "type": "string",
                "required": False,
                "description": "only return items newer than this (ISO-8601 or relative like '2 days ago'; items with no date are skipped when set)",
            },
            {
                "name": "timeout",
                "type": "integer",
                "default": DEFAULT_TIMEOUT,
                "description": "HTTP timeout in seconds",
            },
        ],
        "outputs": [
            {
                "name": "items",
                "type": "list",
                "description": "feed items",
            },
            {
                "name": "count",
                "type": "integer",
                "description": "number of items returned after filtering",
            },
            {
                "name": "markdown",
                "type": "string",
                "description": "markdown digest suitable for piping into an LLM",
            },
        ],
    }


def parse_from_date(value):
    """parses either a relative ('2 days ago') or ISO-8601 string into an aware UTC datetime."""
    if not value:
        return None
    s = str(value).strip()
    if not s:
        return None

    m = RELATIVE_RE.match(s)
    if m:
        amount = int(m.group(1))
        unit = m.group(2).lower()
        delta = timedelta(seconds=amount * RELATIVE_UNITS[unit])
        return datetime.now(timezone.utc) - delta

    # absolute ISO-8601; tolerate trailing Z
    iso = s.replace("Z", "+00:00") if s.endswith("Z") else s
    try:
        dt = datetime.fromisoformat(iso)
    except ValueError as e:
        raise ValueError(f"could not parse from_date '{value}': {e}")
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)


def parse_feed_date(value):
    """parses a feed-provided date string (RFC 822 or ISO-8601) into aware UTC, or None."""
    if not value:
        return None
    s = value.strip()
    # try RFC 822 (RSS 2.0 pubDate)
    try:
        dt = parsedate_to_datetime(s)
        if dt is not None:
            if dt.tzinfo is None:
                dt = dt.replace(tzinfo=timezone.utc)
            return dt.astimezone(timezone.utc)
    except (TypeError, ValueError):
        pass
    # fall back to ISO-8601 (Atom)
    try:
        iso = s.replace("Z", "+00:00") if s.endswith("Z") else s
        dt = datetime.fromisoformat(iso)
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return dt.astimezone(timezone.utc)
    except ValueError:
        return None


def fetch_feed(url, timeout):
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return resp.read()


def find_text(element, *tags):
    """returns stripped text of the first matching child tag, or empty string."""
    for tag in tags:
        child = element.find(tag)
        if child is not None and child.text:
            return child.text.strip()
    return ""


def extract_rss_items(root):
    """extracts items from an RSS 2.0 channel element. returns (feed_title, [items])."""
    channel = root.find("channel")
    if channel is None:
        return "", []
    feed_title = find_text(channel, "title")
    items = []
    for el in channel.findall("item"):
        items.append(
            {
                "title": find_text(el, "title"),
                "link": find_text(el, "link"),
                "comments": find_text(el, "comments"),
                "published": find_text(el, "pubDate", "date"),
                "author": find_text(
                    el, "author", "{http://purl.org/dc/elements/1.1/}creator"
                ),
                "summary": find_text(el, "description", "summary"),
            }
        )

    return feed_title, items


def extract_atom_link(entry):
    """atom links live in <link href='...'/>, preferring rel='alternate'."""
    alternate = None
    first = None
    for link in entry.findall(f"{ATOM_NS}link"):
        href = link.attrib.get("href", "")
        if not href:
            continue
        if first is None:
            first = href
        if link.attrib.get("rel", "alternate") == "alternate":
            alternate = href
            break
    return alternate or first or ""


def extract_atom_items(root):
    """extracts entries from an Atom feed. returns (feed_title, [items])."""
    feed_title = find_text(root, f"{ATOM_NS}title")
    items = []
    for entry in root.findall(f"{ATOM_NS}entry"):
        author_el = entry.find(f"{ATOM_NS}author/{ATOM_NS}name")
        items.append(
            {
                "title": find_text(entry, f"{ATOM_NS}title"),
                "link": extract_atom_link(entry),
                "comments": "",
                "published": find_text(
                    entry, f"{ATOM_NS}published", f"{ATOM_NS}updated"
                ),
                "author": (
                    author_el.text.strip()
                    if author_el is not None and author_el.text
                    else ""
                ),
                "summary": find_text(entry, f"{ATOM_NS}summary", f"{ATOM_NS}content"),
            }
        )
    return feed_title, items


def parse_feed_bytes(data):
    """parses feed bytes into (feed_title, items). detects RSS vs Atom from root tag."""
    root = ET.fromstring(data)
    if root.tag.endswith("rss") or root.tag == "rss":
        return extract_rss_items(root)
    if root.tag == f"{ATOM_NS}feed" or root.tag.endswith("feed"):
        return extract_atom_items(root)
    raise ValueError(f"unknown feed root element: {root.tag}")


def normalize_urls(value):
    """accepts a list of strings, a single string, or newline/comma-separated string."""
    if value is None:
        return []
    if isinstance(value, list):
        return [str(u).strip() for u in value if str(u).strip()]
    s = str(value).strip()
    if not s:
        return []
    parts = re.split(r"[\n,]", s)
    return [p.strip() for p in parts if p.strip()]


def format_digest_item(item):
    parts = [f"## {item['title'] or '(untitled)'}"]
    meta_bits = []
    if item.get("published_human"):
        meta_bits.append(item["published_human"])
    if item.get("author"):
        meta_bits.append(item["author"])
    meta_line = " — ".join(meta_bits)
    if item.get("link"):
        link_frag = f"[source]({item['link']})"
        meta_line = f"*{meta_line}* · {link_frag}" if meta_line else link_frag
    elif meta_line:
        meta_line = f"*{meta_line}*"
    if meta_line:
        parts.append(meta_line)
    if item.get("summary"):
        parts.append(item["summary"])
    parts.append("---")
    return "\n\n".join(parts)


def build_digest(grouped):
    """grouped is a list of (feed_title, [items]) in preserved order."""
    sections = []
    for feed_title, items in grouped:
        if not items:
            continue
        header = f"# {feed_title or 'Feed'} ({len(items)} item{'s' if len(items) != 1 else ''})"
        body = "\n\n".join(format_digest_item(i) for i in items)
        sections.append(f"{header}\n\n{body}")
    return "\n\n".join(sections)


def execute(inputs):
    urls = normalize_urls(inputs.get("url"))
    if not urls:
        raise ValueError("FetchRSS: at least one url is required")

    timeout = int(inputs.get("timeout") or DEFAULT_TIMEOUT)
    from_dt = parse_from_date(inputs.get("from_date"))

    all_items = []
    grouped = []
    for url in urls:
        try:
            data = fetch_feed(url, timeout)
            feed_title, raw_items = parse_feed_bytes(data)
        except Exception as e:
            log(f"FetchRSS: failed to fetch/parse {url}: {e}")
            raise

        kept = []
        for raw in raw_items:
            dt = parse_feed_date(raw["published"])
            if from_dt is not None:
                if dt is None or dt < from_dt:
                    continue
            enriched = dict(raw)
            enriched["feed"] = feed_title
            enriched["published_dt"] = dt
            enriched["published_human"] = (
                dt.strftime("%Y-%m-%d %H:%M UTC") if dt else ""
            )
            kept.append(enriched)
        grouped.append((feed_title, kept))
        all_items.extend(kept)

    # sort newest first; items without dates go last
    all_items.sort(
        key=lambda i: (
            i["published_dt"] is None,
            -(i["published_dt"].timestamp() if i["published_dt"] else 0),
        )
    )

    # strip internal fields before emitting
    public_items = [
        {
            "title": i["title"],
            "link": i["link"],
            "comments": i["comments"],
            "published": i["published_human"],
            "author": i["author"],
            "summary": i["summary"],
            "feed": i["feed"],
        }
        for i in all_items
    ]

    digest = build_digest(grouped)

    return {
        "items": public_items,
        "count": len(public_items),
        "markdown": digest,
    }

"""Validates a JSON document against a DATEX II JSON schema set, with `jsonschema`.

Usage: validate_json.py <schema folder> <entry schema file> < document.json

The schema files reference each other by file name; every file of the folder is registered under
its name. Exit status 0 when the document is valid; otherwise the first errors are printed.
This is the independent judge of what the datex2 JSON writer produces.
"""

import json
import pathlib
import sys

import jsonschema
from referencing import Registry, Resource
from referencing.jsonschema import DRAFT202012


def main() -> int:
    folder = pathlib.Path(sys.argv[1])
    entry = sys.argv[2]
    resources = []
    for path in sorted(folder.glob("*.json")):
        contents = json.loads(path.read_text(encoding="utf-8"))
        if "$schema" not in contents:
            continue  # an example instance, not a schema
        resources.append((path.name, Resource.from_contents(contents, default_specification=DRAFT202012)))
    registry = Registry().with_resources(resources)
    schema = registry.contents(entry)
    validator = jsonschema.Draft202012Validator(schema, registry=registry)
    document = json.load(sys.stdin)
    errors = sorted(validator.iter_errors(document), key=lambda e: list(e.absolute_path))
    for error in errors[:5]:
        where = "/".join(str(p) for p in error.absolute_path)
        print(f"{where}: {error.message[:300]}")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())

"""Check the proposed SG08 identity vectors using only Python's standard library."""
from __future__ import annotations

import copy
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VECTORS = ROOT / "qualification" / "sg08-identity-vectors.json"
MAX_INPUT_BYTES = 1024 * 1024
MAX_ENCODED_BYTES = 1024 * 1024
MAX_DEPTH = 32
MAX_NODES = 4096
MAX_CONTAINER_ENTRIES = 1024
DOMAIN = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\Z")
DOMAINS = {
    "splicery-recipe",
    "splicery-capture",
    "splicery-candidate",
    "splicery-prepared-edit",
    "splicery-request",
    "splicery-effect",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def object_without_duplicates(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load(path):
    require(path.stat().st_size <= MAX_INPUT_BYTES, "identity vector input is too large")
    text = path.read_bytes().decode("utf-8")
    return json.loads(text, object_pairs_hook=object_without_duplicates)


def header(major, argument):
    require(type(argument) is int and argument >= 0, "invalid CBOR header argument")
    if argument <= 23:
        return bytes([(major << 5) | argument])
    if argument <= 0xFF:
        return bytes([(major << 5) | 24, argument])
    if argument <= 0xFFFF:
        return bytes([(major << 5) | 25]) + argument.to_bytes(2, "big")
    if argument <= 0xFFFFFFFF:
        return bytes([(major << 5) | 26]) + argument.to_bytes(4, "big")
    require(argument <= 0xFFFFFFFFFFFFFFFF, "CBOR integer exceeds u64")
    return bytes([(major << 5) | 27]) + argument.to_bytes(8, "big")


def encode(value, depth=0, nodes=None):
    if nodes is None:
        nodes = [0]
    nodes[0] += 1
    require(nodes[0] <= MAX_NODES, "CBOR node budget exceeded")
    require(depth < MAX_DEPTH, "CBOR depth budget exceeded")
    if value is None:
        encoded = b"\xf6"
    elif value is True:
        encoded = b"\xf5"
    elif value is False:
        encoded = b"\xf4"
    elif type(value) is int:
        require(value >= 0, "negative fixture integers are outside this vector profile")
        encoded = header(0, value)
    elif isinstance(value, str):
        raw = value.encode("utf-8")
        encoded = header(3, len(raw)) + raw
    elif isinstance(value, list):
        require(len(value) <= MAX_CONTAINER_ENTRIES, "CBOR array budget exceeded")
        encoded = header(4, len(value)) + b"".join(
            encode(item, depth + 1, nodes) for item in value
        )
    elif isinstance(value, dict):
        if set(value) == {"$bytes_hex"}:
            text = value["$bytes_hex"]
            require(isinstance(text, str) and re.fullmatch(r"(?:[0-9a-f]{2})*", text) is not None,
                    "byte string must be lowercase, even-length hex")
            raw = bytes.fromhex(text)
            encoded = header(2, len(raw)) + raw
        else:
            require(len(value) <= MAX_CONTAINER_ENTRIES, "CBOR map budget exceeded")
            require(all(isinstance(key, str) for key in value), "CBOR map key is not text")
            entries = sorted(value.items(), key=lambda item: (len(item[0].encode("utf-8")), item[0].encode("utf-8")))
            encoded = header(5, len(entries)) + b"".join(
                encode(key, depth + 1, nodes) + encode(item, depth + 1, nodes)
                for key, item in entries
            )
    else:
        raise ValueError(f"unsupported fixture value type: {type(value).__name__}")
    require(len(encoded) <= MAX_ENCODED_BYTES, "CBOR output budget exceeded")
    return encoded


def fingerprint(domain, payload):
    require(isinstance(domain, str) and DOMAIN.fullmatch(domain), "invalid identity domain")
    canonical = encode(payload)
    digest = hashlib.sha256()
    digest.update(b"writfold-engineering/1/")
    digest.update(domain.encode("ascii"))
    digest.update(b"\0")
    digest.update(canonical)
    return canonical, digest.hexdigest()


def by_name(document, name):
    matches = [row for row in document["vectors"] if row["name"] == name]
    require(len(matches) == 1, f"missing or duplicate vector: {name}")
    return matches[0]


def check_goldens(document):
    require(document["schema"] == "splicery.sg08-identity-vectors/1", "unknown vector schema")
    require(document["status"] == "owner_review_required", "proposal was promoted without owner review")
    require(set(document["domains"]) == DOMAINS, "identity domain registry mismatch")
    names = [row["name"] for row in document["vectors"]]
    require(len(names) == len(set(names)), "duplicate vector name")
    for row in document["vectors"]:
        canonical, digest = fingerprint(row["domain"], row["payload"])
        require(canonical.hex() == row["canonical_cbor_hex"], f"canonical bytes differ: {row['name']}")
        require(digest == row["sha256"], f"digest differs: {row['name']}")
        require(re.fullmatch(r"[0-9a-f]{64}", digest) is not None, "noncanonical digest encoding")
    calibration = by_name(document, "writfold-dependency-lock-calibration")
    require(calibration["domain"] == "dependency-lock", "Writfold calibration domain drift")
    require(calibration["source_fingerprint"] == f"wf1:dependency-lock:sha256:{calibration['sha256']}",
            "Writfold calibration fingerprint provenance drift")
    return len(document["vectors"])


def check_fixture_and_references(document):
    fixture = document["fixture"]
    recipe_text = fixture["recipe_utf8"].encode("utf-8")
    original = bytes.fromhex(fixture["original_hex"])
    candidate = bytes.fromhex(fixture["candidate_hex"])
    lo, hi = fixture["target"]
    replacement = bytes.fromhex(fixture["replacement_hex"])
    require(original[:lo] + replacement + original[hi:] == candidate, "candidate is not the exact fixture splice")

    recipe = by_name(document, "recipe")["payload"]
    capture = by_name(document, "capture")["payload"]
    candidate_row = by_name(document, "candidate")
    prepared = by_name(document, "prepared-edit")
    request = by_name(document, "request")
    effect = by_name(document, "effect")
    require(recipe["source_hex"] == recipe_text.hex(), "recipe bytes differ from fixture")
    require(recipe["source_sha256"] == hashlib.sha256(recipe_text).hexdigest(), "recipe content digest mismatch")
    require(capture["content_sha256"] == hashlib.sha256(original).hexdigest(), "capture content digest mismatch")
    require(capture["content_length"] == len(original), "capture content length mismatch")
    require(candidate_row["payload"]["content_sha256"] == hashlib.sha256(candidate).hexdigest(), "candidate content digest mismatch")
    require(candidate_row["payload"]["content_length"] == len(candidate), "candidate content length mismatch")
    require(candidate_row["payload"]["capture"] == by_name(document, "capture")["sha256"], "candidate capture reference mismatch")
    payload = prepared["payload"]
    require(payload["recipe"] == by_name(document, "recipe")["sha256"], "prepared recipe reference mismatch")
    require(payload["capture"] == by_name(document, "capture")["sha256"], "prepared capture reference mismatch")
    require(payload["candidate"] == candidate_row["sha256"], "prepared candidate reference mismatch")
    require(payload["read_set"] == [by_name(document, "capture")["sha256"]], "prepared read set mismatch")
    require(payload["normalized_splices"] == [{"target": [lo, hi], "replacement_hex": replacement.hex()}], "prepared splice mismatch")
    require(request["payload"]["intent"]["prepared_edit"] == prepared["sha256"], "request intent reference mismatch")
    require(effect["payload"]["request"] == request["sha256"], "effect request reference mismatch")
    require(effect["payload"]["observation"]["before_capture"] == by_name(document, "capture")["sha256"], "effect preimage mismatch")
    require(effect["payload"]["observation"]["after_candidate"] == candidate_row["sha256"], "effect postimage mismatch")


def check_negative_controls(document):
    recipe = by_name(document, "recipe")
    _, original_digest = fingerprint("splicery-recipe", recipe["payload"])
    _, substituted_digest = fingerprint("splicery-candidate", recipe["payload"])
    require(original_digest != substituted_digest, "domain substitution did not change digest")

    reordered = dict(reversed(list(recipe["payload"].items())))
    require(encode(reordered) == encode(recipe["payload"]), "map insertion order changed canonical bytes")

    calibration = by_name(document, "writfold-dependency-lock-calibration")
    reordered_array = copy.deepcopy(calibration["payload"])
    reordered_array["edges"].reverse()
    require(fingerprint(calibration["domain"], reordered_array)[1] != calibration["sha256"], "array order did not change digest")

    composed = fingerprint("splicery-recipe", {"schema_version": 1, "text": "é"})[1]
    decomposed = fingerprint("splicery-recipe", {"schema_version": 1, "text": "e\u0301"})[1]
    require(composed != decomposed, "Unicode normalization changed exact-text identity")

    empty = fingerprint("splicery-prepared-edit", {"schema_version": 1, "read_set": []})[1]
    omitted = fingerprint("splicery-prepared-edit", {"schema_version": 1})[1]
    require(empty != omitted, "empty list and omitted field have identical identity")

    request = by_name(document, "request")
    aba = copy.deepcopy(request["payload"])
    aba["intent"]["resource_generation"] += 1
    _, aba_digest = fingerprint(request["domain"], aba)
    require(aba["request_key"] == request["payload"]["request_key"], "ABA control changed request key")
    require(aba_digest != request["sha256"], "byte-ABA generation change did not change intent digest")

    try:
        json.loads('{"schema_version":1,"schema_version":2}', object_pairs_hook=object_without_duplicates)
    except ValueError:
        pass
    else:
        raise ValueError("duplicate JSON keys were accepted")
    return [
        "domain-substitution",
        "map-order-canonicalization",
        "array-order-material",
        "exact-unicode-text",
        "empty-list-vs-omission",
        "request-key-byte-aba-conflict",
        "duplicate-json-key-refusal",
    ]


def main():
    document = load(VECTORS)
    golden_count = check_goldens(document)
    check_fixture_and_references(document)
    controls = check_negative_controls(document)
    require(document["negative_controls"] == controls, "negative control registry drift")
    print(json.dumps({
        "status": "passed_sg08_identity_proposal_component",
        "scope": "owner-review identity vectors only; no RPC, authority, write, or SG08 gate pass",
        "golden_vectors": golden_count,
        "writfold_calibration": "passed",
        "fixture_and_cross_references": "passed",
        "negative_controls": controls,
    }, indent=2))


if __name__ == "__main__":
    main()

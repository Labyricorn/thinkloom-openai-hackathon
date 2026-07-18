import { createHash } from "node:crypto";

function normalizedString(value) {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw new TypeError("Canonical JSON prohibits unpaired Unicode surrogates.");
      index += 1;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      throw new TypeError("Canonical JSON prohibits unpaired Unicode surrogates.");
    }
  }
  return value.normalize("NFC");
}

function normalize(value) {
  if (typeof value === "string") return normalizedString(value);
  if (typeof value === "number") {
    if (!Number.isFinite(value)) throw new TypeError("Canonical JSON prohibits non-finite numbers.");
    return value;
  }
  if (value === null || typeof value === "boolean") return value;
  if (["undefined", "bigint", "symbol", "function"].includes(typeof value)) {
    throw new TypeError(`Canonical JSON prohibits ${typeof value} values.`);
  }
  if (Array.isArray(value)) {
    const enumerableKeys = Object.keys(value);
    if (enumerableKeys.length !== value.length || enumerableKeys.some((key, index) => key !== String(index))) {
      throw new TypeError("Canonical JSON prohibits sparse arrays and non-index array properties.");
    }
    return value.map(normalize);
  }
  if (value && typeof value === "object") {
    const prototype = Object.getPrototypeOf(value);
    if (prototype !== Object.prototype && prototype !== null) throw new TypeError("Canonical JSON accepts only plain objects.");
    if (Reflect.ownKeys(value).length !== Object.keys(value).length) throw new TypeError("Canonical JSON prohibits symbol and non-enumerable object keys.");
    const normalized = Object.create(null);
    for (const [key, item] of Object.entries(value)) {
      const normalizedKey = normalizedString(key);
      if (Object.prototype.hasOwnProperty.call(normalized, normalizedKey)) {
        throw new TypeError(`Canonical JSON key collision after NFC normalization: ${normalizedKey}`);
      }
      normalized[normalizedKey] = normalize(item);
    }
    return normalized;
  }
  throw new TypeError(`Canonical JSON prohibits ${typeof value} values.`);
}

function serialize(value) {
  if (value === null || typeof value === "boolean" || typeof value === "number" || typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(serialize).join(",")}]`;
  const keys = Object.keys(value).sort();
  return `{${keys.map((key) => `${JSON.stringify(key)}:${serialize(value[key])}`).join(",")}}`;
}

export function canonicalize(value) {
  return serialize(normalize(value));
}

export function sha256Bytes(value) {
  return createHash("sha256").update(value).digest();
}

export function sha256(value) {
  const input = typeof value === "string" || Buffer.isBuffer(value) ? value : canonicalize(value);
  return `sha256:${sha256Bytes(input).toString("hex")}`;
}

export function eventIdentity(event) {
  const identity = { ...event };
  delete identity.event_hash;
  return identity;
}

export function hashEvent(event) {
  return sha256(eventIdentity(event));
}

function digestBytes(value) {
  const match = /^sha256:([a-f0-9]{64})$/.exec(value);
  if (!match) throw new TypeError(`Invalid SHA-256 digest: ${value}`);
  return Buffer.from(match[1], "hex");
}

function releaseLeaf(entry) {
  const domain = Buffer.from("thinkloom-release-leaf-v1\0", "utf8");
  const fields = Buffer.from(`${entry.path}\0${entry.size}\0`, "utf8");
  return sha256Bytes(Buffer.concat([domain, fields, digestBytes(entry.sha256)]));
}

export function releaseMerkleRoot(entries) {
  const ordered = [...entries].sort((left, right) => Buffer.from(left.path, "utf8").compare(Buffer.from(right.path, "utf8")));
  if (!ordered.length) return `sha256:${sha256Bytes("thinkloom-release-empty-v1\0").toString("hex")}`;
  let level = ordered.map(releaseLeaf);
  const domain = Buffer.from("thinkloom-release-node-v1\0", "utf8");
  while (level.length > 1) {
    const next = [];
    for (let index = 0; index < level.length; index += 2) {
      const left = level[index];
      const right = level[index + 1] ?? left;
      next.push(sha256Bytes(Buffer.concat([domain, left, right])));
    }
    level = next;
  }
  return `sha256:${level[0].toString("hex")}`;
}

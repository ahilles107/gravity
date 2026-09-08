export type VersionOrder = "older" | "same" | "newer" | "unknown";

interface SemanticVersion {
  readonly major: string;
  readonly minor: string;
  readonly patch: string;
  readonly prerelease: readonly string[];
}

const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
const NUMERIC = /^\d+$/;

function parseSemanticVersion(value: string): SemanticVersion | null {
  const match = SEMVER.exec(value);
  const major = match?.[1];
  const minor = match?.[2];
  const patch = match?.[3];
  const prereleaseText = match?.[4];
  if (major === undefined || minor === undefined || patch === undefined) {
    return null;
  }
  const prerelease = prereleaseText === undefined ? [] : prereleaseText.split(".");
  if (prerelease.some((part) => NUMERIC.test(part) && part.length > 1 && part.startsWith("0"))) {
    return null;
  }
  return { major, minor, patch, prerelease };
}

function compareNumeric(left: string, right: string): -1 | 0 | 1 {
  if (left.length !== right.length) {
    return left.length < right.length ? -1 : 1;
  }
  if (left === right) {
    return 0;
  }
  return left < right ? -1 : 1;
}

/** Orders one prerelease identifier; a numeric one always ranks below an alphanumeric one. */
function compareIdentifier(left: string, right: string): -1 | 0 | 1 {
  if (left === right) {
    return 0;
  }
  const leftNumeric = NUMERIC.test(left);
  const rightNumeric = NUMERIC.test(right);
  if (leftNumeric !== rightNumeric) {
    return leftNumeric ? -1 : 1;
  }
  if (leftNumeric) {
    return compareNumeric(left, right);
  }
  return left < right ? -1 : 1;
}

/** Orders prerelease identifiers; an absent prerelease ranks above any present one. */
function comparePrerelease(left: readonly string[], right: readonly string[]): -1 | 0 | 1 {
  if (left.length === 0 && right.length === 0) {
    return 0;
  }
  if (left.length === 0 || right.length === 0) {
    return left.length === 0 ? 1 : -1;
  }

  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const leftPart = left[index];
    const rightPart = right[index];
    // The shorter identifier list ranks lower once the shared prefix ties.
    if (leftPart === undefined || rightPart === undefined) {
      return leftPart === undefined ? -1 : 1;
    }
    const order = compareIdentifier(leftPart, rightPart);
    if (order !== 0) {
      return order;
    }
  }
  return 0;
}

/** Compares two semantic versions without making unknown formats update-safe. */
export function compareVersions(left: string, right: string): VersionOrder {
  const parsedLeft = parseSemanticVersion(left);
  const parsedRight = parseSemanticVersion(right);
  if (parsedLeft === null || parsedRight === null) {
    return "unknown";
  }

  for (const [leftPart, rightPart] of [
    [parsedLeft.major, parsedRight.major],
    [parsedLeft.minor, parsedRight.minor],
    [parsedLeft.patch, parsedRight.patch],
  ]) {
    const order = compareNumeric(leftPart, rightPart);
    if (order !== 0) {
      return order < 0 ? "older" : "newer";
    }
  }

  const prereleaseOrder = comparePrerelease(parsedLeft.prerelease, parsedRight.prerelease);
  if (prereleaseOrder === 0) {
    return "same";
  }
  return prereleaseOrder < 0 ? "older" : "newer";
}

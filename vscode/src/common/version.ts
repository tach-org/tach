export class VersionInfo {
    constructor(
        public major: number,
        public minor: number,
        public patch: number
    ) {}

    toString(): string {
        return `${this.major}.${this.minor}.${this.patch}`;
    }
}

function versionGte(a: VersionInfo, b: VersionInfo): boolean {
    if (a.major !== b.major) {
        return a.major > b.major;
    }
    if (a.minor !== b.minor) {
        return a.minor > b.minor;
    }
    return a.patch >= b.patch;
}
  
const MIN_VERSION_WITH_CONFIG = {
    major: 0,
    minor: 25,
    patch: 2,
};



export function supportsCustomConfig(version: VersionInfo): boolean {
    return versionGte(version, MIN_VERSION_WITH_CONFIG);
}

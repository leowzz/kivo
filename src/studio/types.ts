import type { ProductDefinition } from "../shared/types";
export type { ProductDefinition, ProductIdentity } from "../shared/types";

export interface StudioError {
  code: string;
  params: Record<string, string>;
  detail: string | null;
}

export interface ProductSummary {
  productVersionId: string;
  displayName: string;
  boardProfileId: string;
  sha256: string | null;
  error: StudioError | null;
}

export interface StudioBoard {
  id: string;
  familyId: string;
  controllerToken: string;
  displayName: string;
  safePins: number[];
  supportsOled: boolean;
}

export interface StudioSnapshot {
  products: ProductSummary[];
  boards: StudioBoard[];
  repoRoot: string;
}

export interface NormalizedDefinition {
  definition: ProductDefinition;
  json: string;
  sha256: string;
  byteLength: number;
}

export interface ProductBuildResult {
  output: {
    outputDirectory: string;
    firmwarePath: string;
    definitionPath: string;
    manifestPath: string;
  };
  logs: string[];
}

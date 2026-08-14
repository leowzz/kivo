import type { HardwareProfile, ModelLayout } from "../types";

export interface ProductIdentity {
  display_name: string;
  family_id: string;
  variant_id: string;
  hardware_revision: number;
  product_version_id: string;
  capabilities: string[];
}

export interface ProductDefinition {
  schema_version: 1;
  product: ProductIdentity;
  layout: ModelLayout;
  hardware_profile: HardwareProfile;
}

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

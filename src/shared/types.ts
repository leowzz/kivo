export interface ModelButton {
  id: string;
  label: string;
}

export interface ButtonGroup {
  id: string;
  columns: number;
  buttons: ModelButton[];
}

export interface ModelLayout {
  id: string;
  name: string;
  groups: ButtonGroup[];
}

export interface DirectInputSource {
  type: "direct";
  id: string;
  keys: Record<string, number>;
}

export interface ContactInputSource {
  type: "contact_matrix";
  id: string;
  pins: number[];
  keys: Record<string, [number, number]>;
}

export interface FeatureSwitchInputSource {
  type: "feature_switch";
  id: string;
  name: string;
  gpio: number;
  buttons: string[];
}

export type InputSource =
  DirectInputSource | ContactInputSource | FeatureSwitchInputSource;

export interface HardwareProfile {
  id: string;
  name: string;
  board_profile_id: string;
  debounce_ms: number;
  ssd1306?: {
    sda: number;
    scl: number;
    control_panel?: {
      type: "ec11_confirm_back";
      confirm: number;
      encoder_press: number;
      encoder_a: number;
      encoder_b: number;
      back: number;
    };
  };
  sh1106?: {
    sda: number;
    scl: number;
    control_panel?: {
      type: "ec11_confirm_back";
      confirm: number;
      encoder_press: number;
      encoder_a: number;
      encoder_b: number;
      back: number;
    };
  };
  inputs: InputSource[];
}

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

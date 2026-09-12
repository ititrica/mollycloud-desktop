import type { GlobalThemeOverrides } from "naive-ui";

// Resolve shared CSS tokens to literals: Naive UI derives colors in JavaScript.
export function createThemeOverrides(): GlobalThemeOverrides {
  const styles = getComputedStyle(document.documentElement);
  const token = (name: string) => styles.getPropertyValue(`--${name}`).trim();
  const border = (name: string) => `1px solid ${token(name)}`;
  const primaryText = token("color-on-lime");
  return {
    common: {
      primaryColor: token("color-lime"), primaryColorHover: token("color-lime-hover"),
      primaryColorPressed: token("color-lime-pressed"), primaryColorSuppl: token("color-lime"),
      infoColor: token("color-blue"), successColor: token("color-lime-deep"),
      warningColor: token("color-warning"), errorColor: token("color-danger"),
      borderRadius: token("radius-md"), fontFamily: token("font-body"), fontFamilyMono: token("font-mono"),
      bodyColor: token("color-bg"), cardColor: token("color-surface"), modalColor: token("color-surface"),
      textColorBase: token("color-text"), textColor1: token("color-text"),
      textColor2: token("color-text-soft"), textColor3: token("color-text-muted"), borderColor: token("color-border"),
    },
    Button: {
      heightSmall: "32px", fontSizeSmall: "13px", borderRadiusSmall: token("radius-sm"),
      heightMedium: "44px", heightLarge: "52px", fontSizeMedium: "14px", fontSizeLarge: "15px",
      borderRadiusMedium: token("radius-md"), borderRadiusLarge: token("radius-md"),
      textColorPrimary: primaryText, textColorHoverPrimary: primaryText,
      textColorPressedPrimary: primaryText, textColorFocusPrimary: primaryText,
      textColorDisabledPrimary: primaryText,
      textColorHover: token("color-lime-deep"), textColorPressed: token("color-on-lime"),
      textColorFocus: token("color-lime-deep"),
      textColorTextPrimary: token("color-lime-deep"), textColorTextHoverPrimary: token("color-on-lime"),
      textColorTextPressedPrimary: token("color-on-lime"), textColorTextFocusPrimary: token("color-lime-deep"),
      colorPrimary: token("color-lime"), colorHoverPrimary: token("color-lime-hover"),
      colorPressedPrimary: token("color-lime-pressed"), colorFocusPrimary: token("color-lime"),
      borderPrimary: border("color-lime"), borderHoverPrimary: border("color-lime-hover"),
      borderPressedPrimary: border("color-lime-pressed"), borderFocusPrimary: border("color-lime-deep"),
    },
    Input: {
      heightLarge: "56px", fontSizeLarge: "16px", paddingLarge: "0 16px",
      borderRadius: token("radius-md"), border: border("color-border-strong"),
      borderHover: border("color-lime-deep"), borderFocus: border("color-lime-deep"),
      boxShadowFocus: token("focus-ring"), placeholderColor: token("color-text-muted"),
    },
    Checkbox: {
      sizeMedium: "16px", fontSizeMedium: "14px", labelLineHeight: "24px",
      labelPadding: "0 0 0 8px", borderRadius: "4px",
      colorChecked: token("color-lime-deep"), border: border("color-border-strong"),
      borderChecked: border("color-lime-deep"), borderFocus: border("color-lime-deep"),
      boxShadowFocus: token("focus-ring"), checkMarkColor: token("color-surface"),
    },
    Switch: { railColor: token("color-surface-strong"), railColorActive: token("color-lime"), buttonColor: token("color-surface") },
    Alert: {
      borderRadius: token("radius-md"), padding: "16px 18px",
      colorWarning: token("color-warning-soft"), borderWarning: border("color-warning-border"),
      colorError: token("color-danger-soft"), borderError: border("color-danger-border"),
    },
    DataTable: { fontSizeMedium: "14px", thFontWeight: "600", thColor: token("color-surface-soft"), tdColor: token("color-surface") },
    Tag: { heightSmall: "28px", heightMedium: "30px", fontSizeSmall: "12px", fontSizeMedium: "13px", borderRadius: "999px" },
    Card: { borderRadius: token("radius-lg"), fontSizeMedium: "14px" },
    Dropdown: { borderRadius: token("radius-md"), fontSizeMedium: "14px" },
  };
}

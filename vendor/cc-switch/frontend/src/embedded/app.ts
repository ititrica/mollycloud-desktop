// This view's About page reports the vendored CC Switch version, not Molly's
// native package version. Avoid api/app's relative import of the iframe core.
export async function getVersion(): Promise<string> {
  return "3.20.3";
}

// Replaced by Vite for each target; a macOS bundle never imports Windows widgets.
export const desktopPlatform = typeof __MOLLY_PLATFORM__ === 'undefined' ? 'macos' : __MOLLY_PLATFORM__;
export const isWindows = desktopPlatform === 'windows';
export const isMacOS = desktopPlatform === 'macos';
export const credentialStoreName = isMacOS ? '应用内加密存储' : isWindows ? 'Windows 凭据管理器' : '系统密钥环';
export const privateKeyStorageName = isMacOS ? '应用私有目录中的加密存储' : isWindows ? 'Windows 加密存储' : '系统密钥环保护的本机加密存储';
export const systemLoginName = isMacOS ? 'macOS' : isWindows ? 'Windows' : '系统';
export const trayName = isMacOS ? '菜单栏' : '托盘';
export const petToggleShortcut = isMacOS ? 'Option+P' : 'Alt+P';
export const imageWorkbenchNativeUrl = isWindows ? 'http://molly-image.localhost/index.html' : 'molly-image://localhost/index.html';

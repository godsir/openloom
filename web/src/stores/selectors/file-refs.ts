const EMPTY_SESSION_FILES: any[] = [];

export function selectSessionFiles(_state: any, _path: string): any[] {
  return EMPTY_SESSION_FILES;
}

export function invalidateSessionCache(_path: string): void {}

export function selectDeskFiles(): any[] {
  return EMPTY_SESSION_FILES;
}

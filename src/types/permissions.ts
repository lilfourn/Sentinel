export interface DirectoryError {
  code: 'NOT_FOUND' | 'NOT_DIRECTORY' | 'PERMISSION_DENIED' | 'READ_ERROR';
  message: string;
  path: string;
  isPermissionError: boolean;
}

export interface ProtectedDirectory {
  name: string;
  path: string;
  accessible: boolean;
}

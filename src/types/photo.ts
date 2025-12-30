export interface PhotoEntry {
  path: string;
  name: string;
  size: number;
  createdAt: number | null;
  modifiedAt: number | null;
  extension: string | null;
}

export interface PhotoGroup {
  id: string;
  label: string;
  photos: PhotoEntry[];
}

export interface PhotoScanResult {
  photos: PhotoEntry[];
  totalCount: number;
  scanDurationMs: number;
  directoriesScanned: number;
}

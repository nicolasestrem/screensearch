import { OCRTextContent, OCRTextData } from '../types';

/**
 * Safely extracts text from OCR content which may have inconsistent structure.
 *
 * The API sometimes returns ocr_text as a string, but occasionally returns it as an object
 * with properties like {text, text_json, x, y, width, height, confidence} or as an array
 * of such objects. This function handles all cases to prevent React rendering errors.
 *
 * @param content - The OCR content from the API response
 * @returns A clean text string safe for rendering
 */
export function getOCRText(content: OCRTextContent): string {
  try {
    if (!content) return '';
    if (typeof content === 'string') return content;

    // Handle single OCR object
    if (typeof content === 'object' && !Array.isArray(content)) {
      const ocrData = content as OCRTextData;

      if (ocrData.text && typeof ocrData.text === 'string') {
        return ocrData.text;
      }

      // Sometimes it might be in text_json
      if (ocrData.text_json) {
        return typeof ocrData.text_json === 'string'
          ? ocrData.text_json
          : JSON.stringify(ocrData.text_json);
      }

      // Return empty string instead of [object Object] to avoid ugly UI
      return '';
    }

    // Handle array of OCR objects
    if (Array.isArray(content)) {
      return content
        .map(item => {
          if (typeof item === 'string') return item;
          if (item && typeof item === 'object' && item.text) return item.text;
          return '';
        })
        .filter(Boolean)
        .join(' ');
    }

    return String(content);
  } catch (e) {
    console.error('Failed to parse OCR text', e);
    return '';
  }
}

/**
 * Type guard to check if a value is OCRTextData
 */
export function isOCRTextData(value: unknown): value is OCRTextData {
  return (
    typeof value === 'object' &&
    value !== null &&
    !Array.isArray(value) &&
    ('text' in value || 'text_json' in value)
  );
}

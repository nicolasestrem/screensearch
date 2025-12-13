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

    // Safety check: if somehow it's a number/boolean
    if (typeof content !== 'object') return String(content);

    // Handle single OCR object
    if (!Array.isArray(content)) {
      const ocrData = content as OCRTextData;

      if (ocrData.text) {
        if (typeof ocrData.text === 'string') return ocrData.text;
        // If text is object, stringify it
        return JSON.stringify(ocrData.text);
      }

      // Sometimes it might be in text_json
      if (ocrData.text_json) {
        return typeof ocrData.text_json === 'string'
          ? ocrData.text_json
          : JSON.stringify(ocrData.text_json);
      }

      // Fallback: If object has no text/text_json but is an object, stringify it to avoid React crash
      return JSON.stringify(ocrData);
    }

    // Handle array of OCR objects
    if (Array.isArray(content)) {
      return content
        .map(item => {
          if (typeof item === 'string') return item;
          if (!item) return '';

          if (typeof item === 'object') {
            if (item.text) {
              return typeof item.text === 'string' ? item.text : JSON.stringify(item.text);
            }
            // If array item is object without text property, stringify it
            return JSON.stringify(item);
          }
          return String(item);
        })
        .filter(Boolean)
        .join(' ');
    }

    return String(content);
  } catch (e) {
    console.error('Failed to parse OCR text', e);
    return ''; // Safe fallback
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

#include "qr_decoder.h"

#include "quirc/quirc.h"
#include "quirc/quirc_internal.h"

#include <cstring>

QrDecoder::QrDecoder()
    : m_qr(quirc_new()) {}

QrDecoder::~QrDecoder() {
    quirc_destroy(m_qr);
}

QStringList QrDecoder::decode(const QImage &image) {
    QStringList result;
    if (!m_qr) {
        return result;
    }

    const QImage grey = image.format() == QImage::Format_Grayscale8
        ? image
        : image.convertToFormat(QImage::Format_Grayscale8);
    const int width = grey.width();
    const int height = grey.height();
    if (width <= 0 || height <= 0 || quirc_resize(m_qr, width, height) < 0) {
        return result;
    }

    uint8_t *raw = quirc_begin(m_qr, nullptr, nullptr);
    if (!raw) {
        return result;
    }
    // QImage scanlines may be padded; quirc requires tightly packed rows.
    for (int y = 0; y < height; ++y) {
        std::memcpy(raw + static_cast<size_t>(y) * width,
                    grey.constScanLine(y), static_cast<size_t>(width));
    }
    quirc_end(m_qr);

    for (int index = 0; index < quirc_count(m_qr); ++index) {
        quirc_code code;
        quirc_extract(m_qr, index, &code);
        quirc_data data;
        if (quirc_decode(&code, &data) == QUIRC_SUCCESS) {
            result.append(QString::fromUtf8(
                reinterpret_cast<const char *>(data.payload), data.payload_len));
        }
    }
    return result;
}

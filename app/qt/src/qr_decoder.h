#pragma once

#include <QImage>
#include <QStringList>

struct quirc;

class QrDecoder {
public:
    QrDecoder();
    ~QrDecoder();

    QrDecoder(const QrDecoder &) = delete;
    QrDecoder &operator=(const QrDecoder &) = delete;

    QStringList decode(const QImage &image);

private:
    quirc *m_qr;
};

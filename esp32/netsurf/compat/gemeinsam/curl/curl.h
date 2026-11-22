/* ESP32-OS: content/fetch.c bindet <curl/curl.h> immer ein (über
 * content/fetchers/curl.h), nutzt curl aber nur mit WITH_CURL. ESP32-OS
 * baut ohne curl (Abrufe über einen eigenen Fetcher, B2): hier nur der
 * Typ für die Deklaration von fetch_curl_multi. */
#ifndef ESP32OS_COMPAT_CURL_H
#define ESP32OS_COMPAT_CURL_H
typedef void CURLM;
#endif

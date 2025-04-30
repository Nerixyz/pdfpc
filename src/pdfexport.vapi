namespace PdfExport {
    [CCode (cheader_filename = "pdf-export.h", cname = "pdf_export_Ctx", cprefix = "pdf_export_ctx_", free_function = "pdf_export_ctx_free", has_type_id = false)]
    [Compact]
    public class Ctx {
        [CCode (cname = "pdf_export_ctx_new")]
        public Ctx(uint8[] path);

        public Error add_image(size_t page, uint8[] data, int32 width, int32 height, float x1, float y1, float x2, float y2);

        public Error page_size(size_t page, out float width, out float height);

        public Error save(uint8[] path);
    }

    [CCode (cheader_filename = "pdf-export.h", cname = "pdf_export_Error", cprefix = "pdf_export_Error_", has_type_id = false)]
    public enum Error {
        Ok,
        BadContext,
        BadPath,
        PageNotFound,
        NoPageObject,
        NoMediaBox,
        NotEnoughMediaBoxPoints,
        FailedToWriteFile,
    }
}

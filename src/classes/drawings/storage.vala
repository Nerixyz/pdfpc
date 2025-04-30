/**
 * Classes to handle drawings over slides.
 *
 * This file is part of pdfpc.
 *
 * Copyright 2017 Charles Reiss
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along
 * with this program; if not, write to the Free Software Foundation, Inc.,
 * 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
 */

namespace pdfpc.Drawings.Storage {

    /**
     * Storage of overlay drawings.
     *
     * This is very similar to caching of slide renderings, except we're likely
     * to want to place the overlays in a user-navigible directory and resize
     * them.
     */
    public abstract class Base {
        /**
         * Metadata object to provide drawing storage for
         */
        protected Metadata.Pdf metadata;

        protected Base(Metadata.Pdf metadata) {
            this.metadata = metadata;
        }

        /**
         * Store an overlay drawing with the given index as an identifier.
         */
        public abstract void store(uint index,
                                   DrawingCommandList drawing_commands);
        /**
         * Retrieve an overlay drawing from storage, or null if none was made.
         *
         * The returned reference can be modified without modifying the storage.
         */
        public abstract pdfpc.DrawingCommandList? retrieve(uint index);

        /**
         * Clear the storage
         */
        public abstract void clear();

        public abstract bool has_any();
        public abstract void save(string path);
    }

    public class MemoryUncompressed : Drawings.Storage.Base {
        /**
         * Actual overlay images.
         */
        protected DrawingCommandList[] drawing_commands_storage = null;

        /**
         * Initialize the storage
         */
        public MemoryUncompressed( Metadata.Pdf metadata ) {
            base(metadata);
            clear();
        }

        public override void store(uint index,
                                   DrawingCommandList drawing_commands) {
            drawing_commands_storage[index] = drawing_commands;
        }

        public override pdfpc.DrawingCommandList? retrieve(uint index) {
            var result = drawing_commands_storage[index];
            drawing_commands_storage[index] = null;
            return result;
        }

        public override void clear() {
            // This is more slots than we might need, but prevents us from being out
            // of bounds if the number of user slides is changed due to overlay marking
            // changing.
            drawing_commands_storage = new pdfpc.DrawingCommandList[this.metadata.get_slide_count()];
        }

        public override bool has_any() {
            for (int i = 0; i < this.metadata.get_slide_count(); i++) {
                if (this.drawing_commands_storage[i] != null && this.drawing_commands_storage[i].drawing_commands.length() != 0) {
                    return true;
                }
            }
            return false;
        }

        public override void save(string path) {
            var ctx = new PdfExport.Ctx((uint8[])this.metadata.pdf_fname.to_utf8());
            for (int i = 0; i < this.metadata.get_slide_count(); i++) {
                if (this.drawing_commands_storage[i] == null || this.drawing_commands_storage[i].drawing_commands.length() == 0) {
                    continue;
                }
                float width_pt, height_pt;
                var err = ctx.page_size(i, out width_pt, out height_pt);
                if (err != PdfExport.Error.Ok) {
                    stderr.printf("Failed to get size for page %d - error %d", i, err);
                    continue;
                }

                // what should this factor be?
                int base_width = (int)(width_pt * 2);
                int base_height = (int)(width_pt * 2);

                double occ_x1, occ_y1, occ_x2, occ_y2;
                this.drawing_commands_storage[i].occupied_rect(out occ_x1, out occ_y1, out occ_x2, out occ_y2);
                int width = (int)((occ_x2 - occ_x1) * (double)base_width);
                int height = (int)((occ_y2 - occ_y1) * (double)base_height);

                Cairo.ImageSurface surface = new Cairo.ImageSurface(Cairo.Format.ARGB32, width, height);
                Cairo.Context cr = new Cairo.Context(surface);
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.0);
                cr.rectangle(0, 0, width, height);
                cr.fill();
                cr.translate(-occ_x1 * (double)base_width, -occ_y1 * (double)base_height);
                this.drawing_commands_storage[i].paint_on_context(cr, base_width, base_height);

                unowned var data = surface.get_data();
                data.length = width * height * 4; // assumes the stride is width * 4
                err = ctx.add_image(i, data, width, height, (float)occ_x1, (float)occ_y1, (float)occ_x2, (float)occ_y2);
                if (err != PdfExport.Error.Ok) {
                    stderr.printf("Failed to add image to page %d - error %d", i, err);
                }
            }

            var err = ctx.save((uint8[])path.to_utf8());
            if (err != PdfExport.Error.Ok) {
                stderr.printf("Failed to save PDF - error %d", err);
            }
        }
    }

    public Storage.Base create(Metadata.Pdf metadata) {
        return new Storage.MemoryUncompressed(metadata);
    }
}

<?php

use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class {
    public function up(): void
    {
        Schema::create('consents', function (Blueprint $table) {
            $table->id();
        });
    }
};
